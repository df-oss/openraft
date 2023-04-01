use std::fmt::Debug;
use std::sync::Arc;

use tokio::sync::oneshot;

use crate::entry::RaftEntry;
use crate::error::Infallible;
use crate::error::InitializeError;
use crate::error::InstallSnapshotError;
use crate::progress::entry::ProgressEntry;
use crate::progress::Inflight;
use crate::raft::AppendEntriesResponse;
use crate::raft::InstallSnapshotResponse;
use crate::raft::VoteRequest;
use crate::raft::VoteResponse;
use crate::EffectiveMembership;
use crate::LogId;
use crate::MetricsChangeFlags;
use crate::Node;
use crate::NodeId;
use crate::SnapshotMeta;
use crate::Vote;

/// Commands to send to `RaftRuntime` to execute, to update the application state.
#[derive(Debug)]
pub(crate) enum Command<NID, N, Ent>
where
    NID: NodeId,
    N: Node,
    Ent: RaftEntry<NID, N>,
{
    /// Becomes a leader, i.e., its `vote` is granted by a quorum.
    /// The runtime initializes leader data when receives this command.
    BecomeLeader,

    /// No longer a leader. Clean up leader's data.
    QuitLeader,

    /// Append one entry.
    AppendEntry { entry: Ent },

    /// Append a `range` of entries.
    AppendInputEntries { entries: Vec<Ent> },

    /// Append a blank log.
    ///
    /// One of the usage is when a leader is established, a blank log is written to commit the
    /// state.
    AppendBlankLog { log_id: LogId<NID> },

    /// Replicate the committed log id to other nodes
    ReplicateCommitted { committed: Option<LogId<NID>> },

    /// Commit entries that are already in the store, upto `upto`, inclusive.
    /// And send applied result to the client that proposed the entry.
    LeaderCommit {
        // TODO: pass the log id list?
        // TODO: merge LeaderCommit and FollowerCommit
        already_committed: Option<LogId<NID>>,
        upto: LogId<NID>,
    },

    /// Commit entries that are already in the store, upto `upto`, inclusive.
    FollowerCommit {
        already_committed: Option<LogId<NID>>,
        upto: LogId<NID>,
    },

    /// Replicate log entries or snapshot to a target.
    Replicate { target: NID, req: Inflight<NID> },

    // /// Replicate a snapshot to a target.
    // ReplicateSnapshot {
    //     target: NID,
    //     snapshot_last_log_id: Option<LogId<NID>>,
    // },
    /// Membership config changed, need to update replication streams.
    UpdateMembership {
        // TODO: not used yet.
        membership: Arc<EffectiveMembership<NID, N>>,
    },

    /// Membership config changed, need to update replication streams.
    /// The Runtime has to close all old replications and start new ones.
    /// Because a replication stream should only report state for one membership config.
    /// When membership config changes, the membership log id stored in ReplicationCore has to be
    /// updated.
    RebuildReplicationStreams {
        /// Targets to replicate to.
        targets: Vec<(NID, ProgressEntry<NID>)>,
    },

    // TODO(3): it also update the progress of a leader.
    //          Add doc:
    //          `target` can also be the leader id.
    /// As the state of replication to `target` is updated, the metrics should be updated.
    UpdateProgressMetrics { target: NID, matching: LogId<NID> },

    /// Save vote to storage
    SaveVote { vote: Vote<NID> },

    /// Send vote to all other members
    SendVote { vote_req: VoteRequest<NID> },

    /// Purge log from the beginning to `upto`, inclusive.
    PurgeLog { upto: LogId<NID> },

    /// Delete logs that conflict with the leader from a follower/learner since log id `since`,
    /// inclusive.
    DeleteConflictLog { since: LogId<NID> },

    /// Install a snapshot data file: e.g., replace state machine with snapshot, save snapshot
    /// data.
    InstallSnapshot { snapshot_meta: SnapshotMeta<NID, N> },

    /// A received snapshot does not need to be installed, just drop buffered snapshot data.
    CancelSnapshot { snapshot_meta: SnapshotMeta<NID, N> },

    // ---
    // --- Response commands
    // ---
    /// Send vote result `res` to its caller via `tx`
    SendVoteResult {
        send: SendResult<Result<VoteResponse<NID>, Infallible>>,
    },

    /// Send append-entries result `res` to its caller via `tx`
    SendAppendEntriesResult {
        send: SendResult<Result<AppendEntriesResponse<NID>, Infallible>>,
    },

    // TODO: use it
    #[allow(dead_code)]
    /// Send install-snapshot result `res` to its caller via `tx`
    SendInstallSnapshotResult {
        send: SendResult<Result<InstallSnapshotResponse<NID>, InstallSnapshotError>>,
    },

    /// Send install-snapshot result `res` to its caller via `tx`
    SendInitializeResult {
        send: SendResult<Result<(), InitializeError<NID, N>>>,
    },

    //
    // --- Draft unimplemented commands:

    // TODO:
    #[allow(dead_code)]
    BuildSnapshot {},
}

/// For unit testing
impl<NID, N, Ent> PartialEq for Command<NID, N, Ent>
where
    NID: NodeId,
    N: Node,
    Ent: RaftEntry<NID, N> + PartialEq,
{
    #[rustfmt::skip]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Command::BecomeLeader,                                Command::BecomeLeader)                                                        => true,
            (Command::QuitLeader,                                  Command::QuitLeader)                                                          => true,
            (Command::AppendEntry { entry },                       Command::AppendEntry { entry: b }, )                                          => entry == b,
            (Command::AppendInputEntries { entries },              Command::AppendInputEntries { entries: b }, )                                 => entries == b,
            (Command::AppendBlankLog { log_id },                   Command::AppendBlankLog { log_id: b }, )                                      => log_id == b,
            (Command::ReplicateCommitted { committed },            Command::ReplicateCommitted { committed: b }, )                               => committed == b,
            (Command::LeaderCommit { already_committed, upto, },   Command::LeaderCommit { already_committed: b_committed, upto: b_upto, }, )    => already_committed == b_committed && upto == b_upto,
            (Command::FollowerCommit { already_committed, upto, }, Command::FollowerCommit { already_committed: b_committed, upto: b_upto, }, )  => already_committed == b_committed && upto == b_upto,
            (Command::Replicate { target, req },                   Command::Replicate { target: b_target, req: other_req, }, )                   => target == b_target && req == other_req,
            (Command::UpdateMembership { membership },             Command::UpdateMembership { membership: b }, )                                => membership == b,
            (Command::RebuildReplicationStreams { targets },       Command::RebuildReplicationStreams { targets: b }, )                          => targets == b,
            (Command::UpdateProgressMetrics { target, matching },  Command::UpdateProgressMetrics { target: b_target, matching: b_matching, }, ) => target == b_target && matching == b_matching,
            (Command::SaveVote { vote },                           Command::SaveVote { vote: b })                                                => vote == b,
            (Command::SendVote { vote_req },                       Command::SendVote { vote_req: b }, )                                          => vote_req == b,
            (Command::PurgeLog { upto },                           Command::PurgeLog { upto: b })                                                => upto == b,
            (Command::DeleteConflictLog { since },                 Command::DeleteConflictLog { since: b }, )                                    => since == b,
            (Command::InstallSnapshot { snapshot_meta },           Command::InstallSnapshot { snapshot_meta: b }, )                              => snapshot_meta == b,
            (Command::CancelSnapshot { snapshot_meta },            Command::CancelSnapshot { snapshot_meta: b }, )                               => snapshot_meta == b,
            (Command::SendVoteResult { send },                     Command::SendVoteResult { send: b })                                          => send == b,
            (Command::SendAppendEntriesResult { send },            Command::SendAppendEntriesResult { send: b })                                 => send == b,
            (Command::SendInstallSnapshotResult { send },          Command::SendInstallSnapshotResult { send: b })                               => send == b,
            (Command::SendInitializeResult { send },               Command::SendInitializeResult { send: b })                                    => send == b,
            (Command::BuildSnapshot {},                            Command::BuildSnapshot {})                                                    => true,
            _ => false,
        }
    }
}

impl<NID, N, Ent> Command<NID, N, Ent>
where
    NID: NodeId,
    N: Node,
    Ent: RaftEntry<NID, N>,
{
    /// Update the flag of the metrics that needs to be updated when this command is executed.
    pub(crate) fn update_metrics_flags(&self, flags: &mut MetricsChangeFlags) {
        match &self {
            Command::BecomeLeader { .. } => flags.set_cluster_changed(),
            Command::QuitLeader => flags.set_cluster_changed(),
            Command::AppendEntry { .. } => flags.set_data_changed(),
            Command::AppendInputEntries { entries } => {
                debug_assert!(!entries.is_empty());
                flags.set_data_changed()
            }
            Command::AppendBlankLog { .. } => flags.set_data_changed(),
            Command::ReplicateCommitted { .. } => {}
            Command::LeaderCommit { .. } => flags.set_data_changed(),
            Command::FollowerCommit { .. } => flags.set_data_changed(),
            Command::Replicate { .. } => {}
            Command::UpdateMembership { .. } => flags.set_cluster_changed(),
            Command::RebuildReplicationStreams { .. } => flags.set_replication_changed(),
            Command::UpdateProgressMetrics { .. } => flags.set_replication_changed(),
            Command::SaveVote { .. } => flags.set_data_changed(),
            Command::SendVote { .. } => {}
            Command::PurgeLog { .. } => flags.set_data_changed(),
            Command::DeleteConflictLog { .. } => flags.set_data_changed(),
            Command::InstallSnapshot { .. } => flags.set_data_changed(),
            Command::CancelSnapshot { .. } => {}
            Command::BuildSnapshot { .. } => flags.set_data_changed(),
            Command::SendVoteResult { .. } => {}
            Command::SendAppendEntriesResult { .. } => {}
            Command::SendInstallSnapshotResult { .. } => {}
            Command::SendInitializeResult { .. } => {}
        }
    }
}

#[derive(Debug)]
pub(crate) struct SendResult<T>
where T: Debug + PartialEq + Eq
{
    res: T,
    tx: oneshot::Sender<T>,
}

impl<T> PartialEq for SendResult<T>
where T: Debug + PartialEq + Eq
{
    fn eq(&self, other: &Self) -> bool {
        self.res == other.res
    }
}

impl<T> Eq for SendResult<T> where T: Debug + PartialEq + Eq {}

impl<T> SendResult<T>
where T: Debug + PartialEq + Eq
{
    pub(crate) fn new(res: T, tx: oneshot::Sender<T>) -> Self {
        Self { res, tx }
    }

    pub(crate) fn send(self) {
        let _ = self.tx.send(self.res);
    }
}
