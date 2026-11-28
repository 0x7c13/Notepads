/// One borrowed IconifyJSON collection admitted by an immutable registry build transaction.
///
/// The bytes and optional registration name are borrowed only while [`super::IconRegistryBuilder`]
/// ingests this value. A successful registry owns all validated state and does not retain either
/// borrow.
#[derive(Clone, Copy)]
pub struct IconPack<'a> {
    json: &'a [u8],
    registration_name: Option<&'a str>,
}

impl std::fmt::Debug for IconPack<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IconPack")
            .field("json_len", &self.json.len())
            .field(
                "registration_name_len",
                &self.registration_name.map(str::len),
            )
            .finish_non_exhaustive()
    }
}

impl<'a> IconPack<'a> {
    pub const fn new(json: &'a [u8]) -> Self {
        Self {
            json,
            registration_name: None,
        }
    }

    #[must_use]
    pub const fn with_registration_name(mut self, registration_name: &'a str) -> Self {
        self.registration_name = Some(registration_name);
        self
    }

    pub const fn json(self) -> &'a [u8] {
        self.json
    }

    pub const fn registration_name(self) -> Option<&'a str> {
        self.registration_name
    }
}
