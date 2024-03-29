#![warn(clippy::all)]

use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

use serde::Deserialize;
use zbus::zvariant::OwnedObjectPath;
use zbus::{zvariant, Connection};

macro_rules! zvar_type {
	($ty:ty, [ $($target:ty),+ ]) => {
		$(
		impl zvariant::Type for $target {
			fn signature() -> zvariant::Signature<'static> {
				<$ty as zvariant::Type>::signature()
			}
		}
		)+
	};
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum StationState {
	Connected,
	Disconnected,
	Connecting,
	Disconnecting,
	Roaming,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
enum DeviceMode {
	AdHoc,
	Station,
	Ap,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum NetworkType {
	Open,
	Wep,
	Psk,
	#[serde(rename = "8021x")]
	Eap,
	Hotspot,
}

zvar_type!(String, [StationState, DeviceMode, NetworkType]);

#[zbus::proxy(
	interface = "org.freedesktop.DBus.ObjectManager",
	gen_blocking = false
)]
trait ObjectManager {
	fn get_managed_objects(
		&self,
	) -> zbus::Result<HashMap<OwnedObjectPath, All>>;
}

#[derive(Debug, zvariant::DeserializeDict)]
#[zvariant(rename_all = "PascalCase")]
struct Station {
	state: StationState,
	connected_network: Option<OwnedObjectPath>,
	scanning: bool,
}

#[derive(Debug, zvariant::DeserializeDict)]
#[zvariant(rename_all = "PascalCase")]
struct Device {
	name: String,
	address: String,
	powered: bool,
	adapter: OwnedObjectPath,
	mode: DeviceMode,
}

#[derive(Debug, zvariant::DeserializeDict)]
#[zvariant(rename_all = "PascalCase")]
struct Network {
	name: String,
	type_: NetworkType,
	connected: bool,
	device: OwnedObjectPath,
	known_network: Option<OwnedObjectPath>,
}

#[derive(Debug, zvariant::DeserializeDict)]
#[zvariant(rename_all = "PascalCase")]
struct KnownNetwork {
	name: String,
	type_: NetworkType,
	hidden: bool,
	last_connected_time: String,
	auto_connect: bool,
}

#[derive(Debug, zvariant::DeserializeDict)]
#[zvariant(rename_all = "PascalCase")]
struct Adapter {
	name: String,
	powered: bool,
	model: Option<String>,
	vendor: Option<String>,
	supported_modes: Box<[DeviceMode]>,
}

#[zbus::interface(name = "net.connman.iwd.Station")]
impl Station {}

#[zbus::interface(name = "net.connman.iwd.Device")]
impl Device {}

#[zbus::interface(name = "net.connman.iwd.Network")]
impl Network {}

#[zbus::interface(name = "net.connman.iwd.KnownNetwork")]
impl KnownNetwork {}

#[zbus::interface(name = "net.connman.iwd.Adapter")]
impl Adapter {}

type Rest = HashMap<
	zbus::names::OwnedInterfaceName,
	HashMap<String, zvariant::OwnedValue>,
>;

#[derive(Default, Debug)]
struct All {
	station: Option<Station>,
	device: Option<Device>,
	network: Option<Network>,
	known_network: Option<KnownNetwork>,
	adapter: Option<Adapter>,
	rest: Rest,
}

zvar_type!(Rest, [All]);

impl<'de> serde::Deserialize<'de> for All {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct Visit;
		impl<'de> serde::de::Visitor<'de> for Visit {
			type Value = All;

			fn expecting(
				&self,
				formatter: &mut std::fmt::Formatter,
			) -> std::fmt::Result {
				formatter.write_str("a map")
			}

			fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
			where
				A: serde::de::MapAccess<'de>,
			{
				let mut res = All::default();
				while let Some(key) = map.next_key()? {
					if key == <Station as zbus::Interface>::name() {
						res.station = Some(map.next_value()?);
					} else if key == <Device as zbus::Interface>::name() {
						res.device = Some(map.next_value()?);
					} else if key == <Network as zbus::Interface>::name() {
						res.network = Some(map.next_value()?);
					} else if key == <KnownNetwork as zbus::Interface>::name() {
						res.known_network = Some(map.next_value()?);
					} else if key == <Adapter as zbus::Interface>::name() {
						res.adapter = Some(map.next_value()?);
					} else {
						res.rest.insert(key, map.next_value()?);
					}
				}
				Ok(res)
			}
		}
		deserializer.deserialize_map(Visit)
	}
}

#[zbus::proxy(
	interface = "net.connman.iwd.Station",
	default_service = "net.connman.iwd",
	gen_blocking = false
)]
trait Station {
	fn scan(&self) -> zbus::Result<()>;

	fn get_ordered_networks(
		&self,
	) -> zbus::Result<Box<[(OwnedObjectPath, i16)]>>;
}

trait FromObjectPath: Sized {
	async fn new(
		conn: &Connection,
		path: OwnedObjectPath,
	) -> zbus::Result<Self>;
}

impl<'a> FromObjectPath for StationProxy<'a> {
	async fn new(
		conn: &Connection,
		path: OwnedObjectPath,
	) -> zbus::Result<Self> {
		Self::new(conn, path).await
	}
}

#[repr(transparent)]
#[derive(Clone)]
struct OPath<T> {
	path: OwnedObjectPath,
	_ty: PhantomData<T>,
}

impl<T> From<OPath<T>> for OwnedObjectPath {
	fn from(value: OPath<T>) -> Self {
		value.path
	}
}

impl<T: FromObjectPath> From<OwnedObjectPath> for OPath<T> {
	fn from(path: OwnedObjectPath) -> Self {
		OPath {
			path,
			_ty: PhantomData,
		}
	}
}

impl<T> fmt::Debug for OPath<T> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.path.as_ref().fmt(f)
	}
}

impl<T> zvariant::Type for OPath<T> {
	#[inline]
	fn signature() -> zvariant::Signature<'static> {
		OwnedObjectPath::signature()
	}
}

impl<T: FromObjectPath> OPath<T> {
	async fn proxy(self, conn: &Connection) -> zbus::Result<T> {
		T::new(conn, self.path).await
	}
}

#[async_std::main]
async fn main() -> anyhow::Result<()> {
	let conn = Connection::system().await?;

	let that = ObjectManagerProxy::new(&conn, "net.connman.iwd", "/").await?;
	let objects = that.get_managed_objects().await?;

	let mut station = None;

	let mut networks = HashMap::new();

	for (path, s) in objects.into_iter() {
		if let All {
			station: Some(_s),
			device: Some(_d),
			..
		} = s
		{
			let path: OPath<StationProxy> = path.into();
			// let connected = s.connected_network.is_some();
			// let scanning = s.scanning;
			// let name = &d.name;
			// println!("{path:?} => name: {name}, connected: {connected}, scanning: {scanning}");
			station = Some(path);
		} else if let All {
			network: Some(network),
			..
		} = s
		{
			networks.insert(path, network);
		} else {
			// println!("{path:?} => {s:#?}");
		}
	}

	if let Some(station) = station {
		dbg!(&station);

		let station = station.proxy(&conn).await?;
		station.scan().await.ok();
		let ordered_networks = station.get_ordered_networks().await?;
		for (net, _strength) in ordered_networks.iter() {
			if let Some(Network {
				// connected,
				// known_network,
				name,
				..
			}) = networks.get(net)
			{
				// let is_known = known_network.is_some();
				// println!("{name} ({connected} {is_known}) {strength}");
				println!("{name}");
			}
		}
	}

	Ok(())
}
