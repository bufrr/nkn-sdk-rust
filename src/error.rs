use std::error::Error;
use std::io;

#[derive(Debug, PartialEq)]
pub enum NKNError {
    Success = 0,
    SessionExpired = 41001,
    ServiceCeiling = 41002,
    IllegalDataFormat = 41003,
    InvalidMethod = 42001,
    InvalidParams = 42002,
    InvalidToken = 42003,
    InvalidTransaction = 43001,
    InvalidAsset = 43002,
    InvalidBlock = 43003,
    InvalidHash = 43004,
    InvalidVersion = 43005,
    UnknownTransaction = 44001,
    UnknownAsset = 44002,
    UnknownBlock = 44003,
    UnknownHash = 44004,
    InternalError = 45001,
    SmartCodeError = 47001,
    WrongNode = 48001,
    NoCode = -2,
    Unknown = -1,
    DuplicatedTx = 1,
    InputOutputTooLong = 45002,
    DuplicateInput = 45003,
    AssetPrecision = 45004,
    TransactionBalance = 45005,
    AttributeProgram = 45006,
    TransactionContracts = 45007,
    TransactionPayload = 45008,
    DoubleSpend = 45009,
    TxHashDuplicate = 45010,
    StateUpdaterVaild = 45011,
    SummaryAsset = 45012,
    XmitFail = 45013,
    DuplicateName = 45015,
    MineReward = 45016,
    DuplicateSubscription = 45017,
    SubscriptionLimit = 45018,
    DoNotPropagate = 45019,
    AlreadySubscribed = 45020,
    AppendTxnPool = 45021,
    NullID = 45022,
    ZeroID = 45023,
    NullDB = 45024,
}

impl From<i64> for NKNError {
    fn from(i: i64) -> Self {
        match i {
            0 => Self::Success,
            41001 => Self::SessionExpired,
            41002 => Self::ServiceCeiling,
            41003 => Self::IllegalDataFormat,
            42001 => Self::InvalidMethod,
            42002 => Self::InvalidParams,
            42003 => Self::InvalidToken,
            43001 => Self::InvalidTransaction,
            43002 => Self::InvalidAsset,
            43003 => Self::InvalidBlock,
            43004 => Self::InvalidHash,
            43005 => Self::InvalidVersion,
            44001 => Self::UnknownTransaction,
            44002 => Self::UnknownAsset,
            44003 => Self::UnknownBlock,
            44004 => Self::UnknownHash,
            45001 => Self::InternalError,
            47001 => Self::SmartCodeError,
            48001 => Self::WrongNode,
            -2 => Self::NoCode,
            -1 => Self::Unknown,
            1 => Self::DuplicatedTx,
            45002 => Self::InputOutputTooLong,
            45003 => Self::DuplicateInput,
            45004 => Self::AssetPrecision,
            45005 => Self::TransactionBalance,
            45006 => Self::AttributeProgram,
            45007 => Self::TransactionContracts,
            45008 => Self::TransactionPayload,
            45009 => Self::DoubleSpend,
            45010 => Self::TxHashDuplicate,
            45011 => Self::StateUpdaterVaild,
            45012 => Self::SummaryAsset,
            45013 => Self::XmitFail,
            45015 => Self::DuplicateName,
            45016 => Self::MineReward,
            45017 => Self::DuplicateSubscription,
            45018 => Self::SubscriptionLimit,
            45019 => Self::DoNotPropagate,
            45020 => Self::AlreadySubscribed,
            45021 => Self::AppendTxnPool,
            45022 => Self::NullID,
            45023 => Self::ZeroID,
            45024 => Self::NullDB,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
struct DoubleError;

#[derive(Debug)]
pub enum NcpError {
    Io(io::Error),
    SessionNotEstablished,
    SessionClosed,
    BufferEmpty,
    BufferTooLarge,
    SessionAlreadyAccepted,
    UndefinedConnection,
}

impl From<io::Error> for NcpError {
    fn from(err: io::Error) -> NcpError {
        NcpError::Io(err)
    }
}

impl std::fmt::Display for NcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            NcpError::Io(ref err) => err.fmt(f),
            _ => f.write_str(self.description()),
        }
    }
}

impl Error for NcpError {
    fn description(&self) -> &str {
        match *self {
            NcpError::Io(ref err) => err.description(),
            NcpError::SessionNotEstablished => "Session is not established",
            NcpError::SessionClosed => "Session is closed",
            NcpError::BufferEmpty => "Buffer is empty",
            NcpError::BufferTooLarge => "Buffer is too large",
            NcpError::SessionAlreadyAccepted => "Session is already accepted",
            NcpError::UndefinedConnection => "The connection is not defined",
        }
    }
}
