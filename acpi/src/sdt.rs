use crate::{AcpiError, AcpiHandler};
use core::{fmt, mem, mem::MaybeUninit, str};

/// Represents a field which may or may not be present within an ACPI structure, depending on the version of ACPI
/// that a system supports. If the field is not present, it is not safe to treat the data as initialised.
#[repr(C, packed)]
pub struct ExtendedField<T: Copy, const MIN_REVISION: u8>(MaybeUninit<T>);

impl<T: Copy, const MIN_REVISION: u8> ExtendedField<T, MIN_REVISION> {
    /// Access the field if it's present for the given revision of the table.
    ///
    /// ### Safety
    /// If a bogus ACPI version is passed, this function may access uninitialised data.
    pub unsafe fn access(&self, revision: u8) -> Option<T> {
        if revision >= MIN_REVISION {
            Some(unsafe { self.0.assume_init() })
        } else {
            None
        }
    }
}

/// All SDTs share the same header, and are `length` bytes long. The signature tells us which SDT
/// this is.
///
/// The ACPI Spec (Version 6.2) defines the following SDT signatures:
///     "APIC" - Multiple APIC Descriptor Table (MADT)
///     "BGRT" - Boot Graphics Resource Table
///     "BERT" - Boot Error Record Table
///     "CPEP" - Corrected Platform Error Polling Table
///     "DSDT" - Differentiated System Descriptor Table
///     "ECDT" - Embedded Controller Boot Resources Table
///     "EINJ" - Error Injection Table
///     "ERST" - Error Record Serialization Table
///     "FACP" - Fixed ACPI Description Table (FADT)
///     "FACS" - Firmware ACPI Control Structure
///     "FPDT" - Firmware Performance Data Table
///     "GTDT" - Generic Timer Description Table
///     "HEST" - Hardware Error Source Table
///     "HMAT" - Heterogeneous Memory Attributes Table
///     "MSCT" - Maximum System Characteristics Table
///     "MPST" - Memory Power State Table
///     "NFIT" - NVDIMM Firmware Interface Table
///     "OEMx" - Various OEM-specific tables
///     "PDTT" - Platform Debug Trigger Table
///     "PMTT" - Platform Memory Topology Table
///     "PPTT" - Processor Properties Topology Table
///     "PSDT" - Persistent System Description Table
///     "RASF" - ACPI RAS Feature Table
///     "RSDT" - Root System Descriptor Table
///     "SBST" - Smart Battery Specification Table
///     "SLIT" - System Locality Information Table
///     "SRAT" - System Resource Affinity Table
///     "SSDT" - Secondary System Description Table
///     "XSDT" - eXtended System Descriptor Table
///
/// We've come across some more ACPI tables in the wild:
///     "WAET" - Windows ACPI Emulated device Table
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct SdtHeader {
    pub signature: Signature,
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

impl SdtHeader {
    /// Check that:
    ///     a) The signature matches the one given
    ///     b) The checksum of the SDT
    ///
    /// This assumes that the whole SDT is mapped.
    pub fn validate(&self, signature: Signature) -> Result<(), AcpiError> {
        // Check the signature
        if self.signature != signature {
            return Err(AcpiError::SdtInvalidSignature(signature));
        }

        // Check the OEM id
        if str::from_utf8(&self.oem_id).is_err() {
            return Err(AcpiError::SdtInvalidOemId(signature));
        }

        // Check the OEM table id
        if str::from_utf8(&self.oem_table_id).is_err() {
            return Err(AcpiError::SdtInvalidTableId(signature));
        }

        // Validate the checksum
        let self_ptr = self as *const SdtHeader as *const u8;
        let mut sum: u8 = 0;
        for i in 0..self.length {
            sum = sum.wrapping_add(unsafe { *(self_ptr.offset(i as isize)) } as u8);
        }

        if sum > 0 {
            return Err(AcpiError::SdtInvalidChecksum(signature));
        }

        Ok(())
    }

    pub fn oem_id<'a>(&'a self) -> &'a str {
        // Safe to unwrap because checked in `validate`
        str::from_utf8(&self.oem_id).unwrap()
    }

    pub fn oem_table_id<'a>(&'a self) -> &'a str {
        // Safe to unwrap because checked in `validate`
        str::from_utf8(&self.oem_table_id).unwrap()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Signature([u8; 4]);

impl Signature {
    pub const RSDT: Signature = Signature(*b"RSDT");
    pub const XSDT: Signature = Signature(*b"XSDT");
    pub const FADT: Signature = Signature(*b"FACP");
    pub const HPET: Signature = Signature(*b"HPET");
    pub const MADT: Signature = Signature(*b"APIC");
    pub const MCFG: Signature = Signature(*b"MCFG");
    pub const SSDT: Signature = Signature(*b"SSDT");

    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.0).unwrap()
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\"", self.as_str())
    }
}

/// Takes the physical address of an SDT, and maps, clones and unmaps its header. Useful for
/// finding out how big it is to map it correctly later.
pub(crate) fn peek_at_sdt_header<H>(handler: &H, physical_address: usize) -> SdtHeader
where
    H: AcpiHandler,
{
    let mapping =
        unsafe { handler.map_physical_region::<SdtHeader>(physical_address, mem::size_of::<SdtHeader>()) };
    (*mapping).clone()
}
