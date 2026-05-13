pub use figment::Figment;
pub use figment::providers::Format;
pub struct SingleOverride<'a> {
  pub key: &'a str,
  pub value: &'a str,
}

impl<'a> figment::Provider for SingleOverride<'a> {
  fn metadata(&self) -> figment::Metadata {
    figment::Metadata::named(format!("CLI Override --set {}={}", self.key, self.value))
  }

  fn data(
    &self,
  ) -> Result<figment::value::Map<figment::Profile, figment::value::Dict>, figment::Error> {
    let val = if let Ok(num) = self.value.parse::<u64>() {
      figment::value::Value::from(num)
    } else if let Ok(b) = self.value.parse::<bool>() {
      figment::value::Value::from(b)
    } else {
      figment::value::Value::from(self.value)
    };
    let nested = figment::util::nest(self.key, val);
    if let figment::value::Value::Dict(_, dict) = nested {
      Ok(figment::Profile::Default.collect(dict))
    } else {
      Ok(figment::value::Map::new())
    }
  }
}

pub struct StripKey<P> {
  pub provider: P,
  pub key: &'static str,
}

impl<P: figment::Provider> figment::Provider for StripKey<P> {
  fn metadata(&self) -> figment::Metadata {
    self.provider.metadata()
  }

  fn data(
    &self,
  ) -> Result<figment::value::Map<figment::Profile, figment::value::Dict>, figment::Error> {
    let mut map = self.provider.data()?;
    for dict in map.values_mut() {
      dict.remove(self.key);
    }
    Ok(map)
  }
}
