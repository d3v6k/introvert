import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart';
import 'rewards_bridge.dart';
import 'package:connectivity_plus/connectivity_plus.dart';

// --- Native C Signatures & Dart Mapping ---

final class FfiResult extends Struct {
  @Int32()
  external int code;
  external Pointer<Uint8> data;
  @Size()
  external int len;

  // Persistent native dummy for safe FFI fallbacks
  static FfiResult get dummy {
    _dummyPtr ??= calloc<FfiResult>()
      ..ref.code = -1
      ..ref.data = nullptr
      ..ref.len = 0;
    return _dummyPtr!.ref;
  }

  static FfiResult success() {
    final res = calloc<FfiResult>();
    res.ref.code = 0;
    res.ref.data = nullptr;
    res.ref.len = 0;
    final val = res.ref;
    calloc.free(res);
    return val;
  }
}

Pointer<FfiResult>? _dummyPtr;

final class MediaFrameHeader extends Struct {
  @Uint8()
  external int codec;
  @Uint32()
  external int width;
  @Uint32()
  external int height;
  @Uint64()
  external int timestamp;
}

typedef NativeNetworkCallback = Void Function(Int32 eventType, Pointer<Uint8> dataPtr, Size dataLen);
typedef NativeFfiCallback = Void Function(FfiResult result);
typedef NativeRewardCallback = Void Function(Int32 status, Pointer<Utf8> txSignature);

typedef IntrovertGenerateMnemonicC = Pointer<Utf8> Function();
typedef IntrovertGenerateMnemonicDart = Pointer<Utf8> Function();

typedef IntrovertFreeStringC = Void Function(Pointer<Utf8> s);
typedef IntrovertFreeStringDart = void Function(Pointer<Utf8> s);

typedef IntrovertFreeBinaryC = Void Function(Pointer<Uint8> ptr, Size len);
typedef IntrovertFreeBinaryDart = void Function(Pointer<Uint8> ptr, int len);

typedef IntrovertMnemonicToSeedC = FfiResult Function(Pointer<Utf8> phrase);
typedef IntrovertMnemonicToSeedDart = FfiResult Function(Pointer<Utf8> phrase);

typedef IntrovertDeriveIdentifiersC = FfiResult Function(Pointer<Uint8> seed);
typedef IntrovertDeriveIdentifiersDart = FfiResult Function(Pointer<Uint8> seed);

typedef IntrovertEngineStartC = FfiResult Function(Pointer<Uint8> seed, Pointer<Utf8> dbPath);
typedef IntrovertEngineStartDart = FfiResult Function(Pointer<Uint8> seed, Pointer<Utf8> dbPath);

typedef IntrovertEngineStopC = FfiResult Function();
typedef IntrovertEngineStopDart = FfiResult Function();

typedef IntrovertGetPeerIdC = Pointer<Utf8> Function();
typedef IntrovertGetPeerIdDart = Pointer<Utf8> Function();

typedef IntrovertNetworkStartC = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback, Uint16 port, Bool relay, Uint32 maxConn, Uint64 liveness);
typedef IntrovertNetworkStartDart = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback, int port, bool relay, int maxConn, int liveness);

typedef IntrovertEconomyStartMonitoringC = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback);
typedef IntrovertEconomyStartMonitoringDart = FfiResult Function(Pointer<NativeFunction<NativeNetworkCallback>> callback);

typedef IntrovertNetworkSendMessageC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Pointer<Utf8> replyTo, Pointer<NativeFunction<NativeFfiCallback>> callback);
typedef IntrovertNetworkSendMessageDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Pointer<Utf8> replyTo, Pointer<NativeFunction<NativeFfiCallback>> callback);

typedef IntrovertNetworkInitiateWebRtcC = FfiResult Function(Pointer<Utf8> peerId, Uint8 mediaType, Pointer<NativeFunction<NativeFfiCallback>> callback);
typedef IntrovertNetworkInitiateWebRtcDart = FfiResult Function(Pointer<Utf8> peerId, int mediaType, Pointer<NativeFunction<NativeFfiCallback>> callback);

typedef IntrovertWebRtcSendNativeSignalC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> json);
typedef IntrovertWebRtcSendNativeSignalDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> json);

typedef IntrovertAddAddressC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> address);
typedef IntrovertAddAddressDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> address);

typedef IntrovertClaimRewardsAsyncC = FfiResult Function(Pointer<NativeFunction<NativeRewardCallback>> callback);
typedef IntrovertClaimRewardsAsyncDart = FfiResult Function(Pointer<NativeFunction<NativeRewardCallback>> callback);

typedef IntrovertStoreMessageAsyncC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, Bool isMe, Pointer<NativeFunction<NativeFfiCallback>> callback);
typedef IntrovertStoreMessageAsyncDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msg, bool isMe, Pointer<NativeFunction<NativeFfiCallback>> callback);

typedef IntrovertStorageGetMessagesC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertStorageGetMessagesDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertStorageGetMessagesPaginatedC = FfiResult Function(Pointer<Utf8> peerId, Uint32 offset, Uint32 limit);
typedef IntrovertStorageGetMessagesPaginatedDart = FfiResult Function(Pointer<Utf8> peerId, int offset, int limit);

typedef IntrovertEstablishSecureSessionC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertEstablishSecureSessionDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertFetchMailboxC = FfiResult Function();
typedef IntrovertFetchMailboxDart = FfiResult Function();

typedef IntrovertStartMediaStreamC = FfiResult Function(Pointer<Utf8> peerId, Uint8 mediaType);
typedef IntrovertStartMediaStreamDart = FfiResult Function(Pointer<Utf8> peerId, int mediaType);

typedef IntrovertStorageGetContactsC = FfiResult Function();
typedef IntrovertStorageGetContactsDart = FfiResult Function();

typedef IntrovertDeleteContactC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertDeleteContactDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertSetProfileTierC = FfiResult Function(Int32 tier);
typedef IntrovertSetProfileTierDart = FfiResult Function(int tier);

typedef IntrovertDeleteChatC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertDeleteChatDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertClearContactsC = FfiResult Function();
typedef IntrovertClearContactsDart = FfiResult Function();

typedef IntrovertWormholeStartC = FfiResult Function();
typedef IntrovertWormholeStartDart = FfiResult Function();

typedef IntrovertWormholeJoinC = FfiResult Function(Pointer<Utf8> code);
typedef IntrovertWormholeJoinDart = FfiResult Function(Pointer<Utf8> code);

typedef IntrovertWormholeAbortC = FfiResult Function();
typedef IntrovertWormholeAbortDart = FfiResult Function();

typedef IntrovertCloseWebRtcC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertCloseWebRtcDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertRenegotiateWebRtcC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertRenegotiateWebRtcDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertAcceptCallC = FfiResult Function(Pointer<Utf8> peerId, Uint8 mediaType);
typedef IntrovertAcceptCallDart = FfiResult Function(Pointer<Utf8> peerId, int mediaType);

typedef IntrovertRejectCallC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertRejectCallDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertSetAnchorModeC = FfiResult Function(Bool enabled);
typedef IntrovertSetAnchorModeDart = FfiResult Function(bool enabled);

typedef IntrovertNetworkSetConnectivityTypeC = FfiResult Function(Uint8 connectivity_type);
typedef IntrovertNetworkSetConnectivityTypeDart = FfiResult Function(int connectivity_type);

typedef IntrovertNetworkSetTunnelModeC = FfiResult Function(Bool enabled);
typedef IntrovertNetworkSetTunnelModeDart = FfiResult Function(bool enabled);
typedef IntrovertNetworkGetTunnelModeC = Int32 Function();
typedef IntrovertNetworkGetTunnelModeDart = int Function();


typedef IntrovertNetworkRecheckConnectionC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertNetworkRecheckConnectionDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertNetworkResolveHandleC = FfiResult Function(Pointer<Utf8> handle);
typedef IntrovertNetworkResolveHandleDart = FfiResult Function(Pointer<Utf8> handle);

typedef IntrovertNetworkSendDirectInviteC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertNetworkSendDirectInviteDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertNetworkRegisterPushTokenC = FfiResult Function(Pointer<Utf8> deviceType, Pointer<Utf8> token);
typedef IntrovertNetworkRegisterPushTokenDart = FfiResult Function(Pointer<Utf8> deviceType, Pointer<Utf8> token);

typedef IntrovertNetworkSetRetentionC = FfiResult Function(Pointer<Utf8> targetId, Uint32 seconds, Bool isGroup);
typedef IntrovertNetworkSetRetentionDart = FfiResult Function(Pointer<Utf8> targetId, int seconds, bool isGroup);

typedef IntrovertNetworkDeleteMessageC = FfiResult Function(Pointer<Utf8> targetId, Pointer<Utf8> msgId, Bool isGroup, Bool deletedByAdmin);
typedef IntrovertNetworkDeleteMessageDart = FfiResult Function(Pointer<Utf8> targetId, Pointer<Utf8> msgId, bool isGroup, bool deletedByAdmin);

typedef IntrovertNetworkEditMessageC = FfiResult Function(Pointer<Utf8> targetId, Pointer<Utf8> msgId, Pointer<Utf8> newContent, Bool isGroup);
typedef IntrovertNetworkEditMessageDart = FfiResult Function(Pointer<Utf8> targetId, Pointer<Utf8> msgId, Pointer<Utf8> newContent, bool isGroup);

typedef IntrovertNetworkSendReactionC = FfiResult Function(Pointer<Utf8> targetId, Pointer<Utf8> msgId, Pointer<Utf8> emoji, Bool isGroup);
typedef IntrovertNetworkSendReactionDart = FfiResult Function(Pointer<Utf8> targetId, Pointer<Utf8> msgId, Pointer<Utf8> emoji, bool isGroup);

typedef IntrovertStorageGetReactionsC = FfiResult Function(Pointer<Utf8> msgId);
typedef IntrovertStorageGetReactionsDart = FfiResult Function(Pointer<Utf8> msgId);

typedef IntrovertNetworkClaimHandleC = FfiResult Function(Pointer<Utf8> handle);
typedef IntrovertNetworkClaimHandleDart = FfiResult Function(Pointer<Utf8> handle);

typedef IntrovertStorageGetHandleStatusC = FfiResult Function(Pointer<Utf8> handle);
typedef IntrovertStorageGetHandleStatusDart = FfiResult Function(Pointer<Utf8> handle);

typedef IntrovertStorageGetLocalHandleC = FfiResult Function();
typedef IntrovertStorageGetLocalHandleDart = FfiResult Function();

typedef IntrovertStorageIsHandleClaimedC = FfiResult Function(Pointer<Utf8> handle);
typedef IntrovertStorageIsHandleClaimedDart = FfiResult Function(Pointer<Utf8> handle);

typedef IntrovertNetworkRequestSwarmStatsC = FfiResult Function();
typedef IntrovertNetworkRequestSwarmStatsDart = FfiResult Function();

typedef IntrovertNetworkPollPeerProfileC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertNetworkPollPeerProfileDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertNetworkSyncChatMessagesC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> chatId, Int32 isGroup, Int32 isFull);
typedef IntrovertNetworkSyncChatMessagesDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> chatId, int isGroup, int isFull);


typedef IntrovertNetworkComputeFileHashC = FfiResult Function(Pointer<Utf8> filePath);
typedef IntrovertNetworkComputeFileHashDart = FfiResult Function(Pointer<Utf8> filePath);

typedef IntrovertGetAnchorModeC = Int32 Function();
typedef IntrovertGetAnchorModeDart = int Function();

typedef IntrovertStorageGetProfileC = FfiResult Function();
typedef IntrovertStorageGetProfileDart = FfiResult Function();

typedef IntrovertStorageSetProfileC = FfiResult Function(Pointer<Utf8> name, Pointer<Utf8> handle, Pointer<Utf8> avatar, Int32 privacyMode);
typedef IntrovertStorageSetProfileDart = FfiResult Function(Pointer<Utf8> name, Pointer<Utf8> handle, Pointer<Utf8> avatar, int privacyMode);

typedef IntrovertNetworkSendFileC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> filePath, Pointer<Utf8> groupId);
typedef IntrovertNetworkSendFileDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> filePath, Pointer<Utf8> groupId);

typedef IntrovertNetworkCancelFileTransferC = FfiResult Function(Pointer<Utf8> transferId);
typedef IntrovertNetworkCancelFileTransferDart = FfiResult Function(Pointer<Utf8> transferId);

typedef IntrovertNetworkForceRefreshC = FfiResult Function();
typedef IntrovertNetworkForceRefreshDart = FfiResult Function();

typedef IntrovertSendManualTelemetryC = FfiResult Function();
typedef IntrovertSendManualTelemetryDart = FfiResult Function();

// --- Intro-Claw AI Engine Mode ---
typedef IntroClawGetAiModeC = Int32 Function();
typedef IntroClawGetAiModeDart = int Function();

typedef IntroClawSetAiModeC = FfiResult Function(Int32 mode, Pointer<Utf8> apiKey);
typedef IntroClawSetAiModeDart = FfiResult Function(int mode, Pointer<Utf8> apiKey);

typedef IntroClawGetApiKeyC = Pointer<Utf8> Function();
typedef IntroClawGetApiKeyDart = Pointer<Utf8> Function();

typedef IntroClawTriggerTickC = FfiResult Function(Bool isMobileData);
typedef IntroClawTriggerTickDart = FfiResult Function(bool isMobileData);

typedef IntroClawSetActiveC = FfiResult Function(Bool active);
typedef IntroClawSetActiveDart = FfiResult Function(bool active);

typedef IntroClawSetNodeModeC = FfiResult Function(Bool enabled);
typedef IntroClawSetNodeModeDart = FfiResult Function(bool enabled);

typedef IntroClawGetStatusC = FfiResult Function();
typedef IntroClawGetStatusDart = FfiResult Function();

typedef IntroClawGetEndpointC = Pointer<Utf8> Function();
typedef IntroClawGetEndpointDart = Pointer<Utf8> Function();

typedef IntroClawSetEndpointC = FfiResult Function(Pointer<Utf8> endpoint);
typedef IntroClawSetEndpointDart = FfiResult Function(Pointer<Utf8> endpoint);

typedef IntroClawProcessQueryC = FfiResult Function(Pointer<Utf8> query);
typedef IntroClawProcessQueryDart = FfiResult Function(Pointer<Utf8> query);

typedef IntroClawRunNetworkReconC = FfiResult Function();
typedef IntroClawRunNetworkReconDart = FfiResult Function();

typedef IntroClawHealPeerC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntroClawHealPeerDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntroClawGetActivityLogC = FfiResult Function();
typedef IntroClawGetActivityLogDart = FfiResult Function();

typedef IntroClawVoipStartCallC = FfiResult Function(Pointer<Utf8> peerId, Int32 isVideo);
typedef IntroClawVoipStartCallDart = FfiResult Function(Pointer<Utf8> peerId, int isVideo);

typedef IntroClawVoipEndCallC = FfiResult Function();
typedef IntroClawVoipEndCallDart = FfiResult Function();

typedef IntroClawVoipRecordSampleC = FfiResult Function(Uint64 rttMs, Double packetLossPct, Uint64 jitterMs, Uint64 bitrateKbps, Int32 isRelayed, Pointer<Utf8> codec);
typedef IntroClawVoipRecordSampleDart = FfiResult Function(int rttMs, double packetLossPct, int jitterMs, int bitrateKbps, int isRelayed, Pointer<Utf8> codec);

typedef IntroClawSetActiveChatC = FfiResult Function(Pointer<Utf8> chatId, Pointer<Utf8> peerId, Int32 isGroup);
typedef IntroClawSetActiveChatDart = FfiResult Function(Pointer<Utf8> chatId, Pointer<Utf8> peerId, int isGroup);

typedef IntroClawClearActiveChatC = FfiResult Function();
typedef IntroClawClearActiveChatDart = FfiResult Function();

typedef IntroClawSetActiveGroupMembersC = FfiResult Function(Pointer<Utf8> membersJson);
typedef IntroClawSetActiveGroupMembersDart = FfiResult Function(Pointer<Utf8> membersJson);

typedef IntroClawOnAppLaunchC = FfiResult Function();
typedef IntroClawOnAppLaunchDart = FfiResult Function();

typedef IntroClawVoipGetQualityC = FfiResult Function();
typedef IntroClawVoipGetQualityDart = FfiResult Function();

typedef IntroClawVoipGetDowngradeRecommendationC = FfiResult Function();
typedef IntroClawVoipGetDowngradeRecommendationDart = FfiResult Function();

typedef IntrovertGroupCreateC = FfiResult Function(Pointer<Utf8> name, Pointer<Utf8> description, Pointer<Utf8> membersJson);
typedef IntrovertGroupCreateDart = FfiResult Function(Pointer<Utf8> name, Pointer<Utf8> description, Pointer<Utf8> membersJson);

typedef IntrovertGroupSendMessageC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> message, Pointer<Utf8> replyTo);
typedef IntrovertGroupSendMessageDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> message, Pointer<Utf8> replyTo);

typedef IntrovertGroupGetAllC = FfiResult Function();
typedef IntrovertGroupGetAllDart = FfiResult Function();

typedef IntrovertGroupGetMessagesC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertGroupGetMessagesDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertGroupAddMemberC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);
typedef IntrovertGroupAddMemberDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);

typedef IntrovertGroupApproveJoinC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId, Pointer<Utf8> alias, Pointer<Utf8> avatar, Pointer<Utf8> handle);
typedef IntrovertGroupApproveJoinDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId, Pointer<Utf8> alias, Pointer<Utf8> avatar, Pointer<Utf8> handle);

typedef IntrovertGroupRejectJoinC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId, Pointer<Utf8> reason);
typedef IntrovertGroupRejectJoinDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId, Pointer<Utf8> reason);

typedef IntrovertGroupRemoveMemberC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);
typedef IntrovertGroupRemoveMemberDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);

typedef IntrovertGroupUpdateRoleC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId, Int32 role);
typedef IntrovertGroupUpdateRoleDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId, int role);

typedef IntrovertGroupPublishManifestC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> code);
typedef IntrovertGroupPublishManifestDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> code);

typedef IntrovertGroupJoinByCodeC = FfiResult Function(Pointer<Utf8> code);
typedef IntrovertGroupJoinByCodeDart = FfiResult Function(Pointer<Utf8> code);

typedef IntrovertGroupDeleteC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertGroupDeleteDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertGroupGetPendingInvitesC = FfiResult Function();
typedef IntrovertGroupGetPendingInvitesDart = FfiResult Function();

typedef IntrovertGroupAcceptInviteC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertGroupAcceptInviteDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertGroupDeclineInviteC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertGroupDeclineInviteDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertGroupMuteMemberC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);
typedef IntrovertGroupMuteMemberDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);

typedef IntrovertGroupUnmuteMemberC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);
typedef IntrovertGroupUnmuteMemberDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> peerId);

typedef IntrovertGroupGetMutedMembersC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertGroupGetMutedMembersDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertNukeIdentityC = FfiResult Function(Pointer<Utf8> dbPath);
typedef IntrovertNukeIdentityDart = FfiResult Function(Pointer<Utf8> dbPath);

typedef IntrovertDriveAddFileC = FfiResult Function(Pointer<Utf8> filename, Pointer<Utf8> fileHash, Pointer<Utf8> mimeType, Int64 size, Pointer<Utf8> localPath);
typedef IntrovertDriveAddFileDart = FfiResult Function(Pointer<Utf8> filename, Pointer<Utf8> fileHash, Pointer<Utf8> mimeType, int size, Pointer<Utf8> localPath);

typedef IntrovertDriveGetAllC = FfiResult Function();
typedef IntrovertDriveGetAllDart = FfiResult Function();

typedef IntrovertDriveGetByHashC = FfiResult Function(Pointer<Utf8> fileHash);
typedef IntrovertDriveGetByHashDart = FfiResult Function(Pointer<Utf8> fileHash);

typedef IntrovertDriveDeleteC = FfiResult Function(Pointer<Utf8> fileHash);
typedef IntrovertDriveDeleteDart = FfiResult Function(Pointer<Utf8> fileHash);

typedef IntrovertDriveAddFileWithFolderC = FfiResult Function(Pointer<Utf8> filename, Pointer<Utf8> fileHash, Pointer<Utf8> mimeType, Int64 size, Pointer<Utf8> localPath, Pointer<Utf8> folder);
typedef IntrovertDriveAddFileWithFolderDart = FfiResult Function(Pointer<Utf8> filename, Pointer<Utf8> fileHash, Pointer<Utf8> mimeType, int size, Pointer<Utf8> localPath, Pointer<Utf8> folder);

typedef IntrovertDriveUpdateFolderC = FfiResult Function(Pointer<Utf8> fileHash, Pointer<Utf8> folder);
typedef IntrovertDriveUpdateFolderDart = FfiResult Function(Pointer<Utf8> fileHash, Pointer<Utf8> folder);

typedef IntrovertGetMeshCapacityC = Int64 Function();
typedef IntrovertGetMeshCapacityDart = int Function();

typedef IntrovertGetDiskSpaceC = Int32 Function(Pointer<Utf8> path, Pointer<Uint64> totalBytes, Pointer<Uint64> freeBytes);
typedef IntrovertGetDiskSpaceDart = int Function(Pointer<Utf8> path, Pointer<Uint64> totalBytes, Pointer<Uint64> freeBytes);

typedef IntrovertNetworkRegisterSeederC = FfiResult Function(Pointer<Utf8> transferId, Pointer<Utf8> filePath, Pointer<Utf8> fileHash, Int64 totalSize, Pointer<Utf8> groupId);
typedef IntrovertNetworkRegisterSeederDart = FfiResult Function(Pointer<Utf8> transferId, Pointer<Utf8> filePath, Pointer<Utf8> fileHash, int totalSize, Pointer<Utf8> groupId);

typedef IntrovertNetworkStartPullC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> transferId, Pointer<Utf8> filename, Pointer<Utf8> mimeType, Pointer<Utf8> fileHash, Int64 totalSize, Bool isRelayed, Pointer<Utf8> groupId);
typedef IntrovertNetworkStartPullDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> transferId, Pointer<Utf8> filename, Pointer<Utf8> mimeType, Pointer<Utf8> fileHash, int totalSize, bool isRelayed, Pointer<Utf8> groupId);

typedef IntrovertStorageGetUnreadCountsC = FfiResult Function();
typedef IntrovertStorageGetUnreadCountsDart = FfiResult Function();

typedef IntrovertStorageUpdateGroupMessageStatusC = FfiResult Function(Pointer<Utf8> groupId, Uint8 status);
typedef IntrovertStorageUpdateGroupMessageStatusDart = FfiResult Function(Pointer<Utf8> groupId, int status);

typedef IntrovertStorageUpdateGroupMessageStatusByIdC = FfiResult Function(Pointer<Utf8> msgId, Uint8 status);
typedef IntrovertStorageUpdateGroupMessageStatusByIdDart = FfiResult Function(Pointer<Utf8> msgId, int status);

typedef IntrovertStorageUpdateMessageStatusForPeerC = FfiResult Function(Pointer<Utf8> peerId, Uint8 status);
typedef IntrovertStorageUpdateMessageStatusForPeerDart = FfiResult Function(Pointer<Utf8> peerId, int status);

// Elevated Messages
typedef IntrovertElevateMessageC = FfiResult Function(Pointer<Utf8> chatId, Pointer<Utf8> msgId, Pointer<Utf8> content, Pointer<Utf8> senderId, Bool isMe);
typedef IntrovertElevateMessageDart = FfiResult Function(Pointer<Utf8> chatId, Pointer<Utf8> msgId, Pointer<Utf8> content, Pointer<Utf8> senderId, bool isMe);

typedef IntrovertUnelevateMessageC = FfiResult Function(Pointer<Utf8> chatId, Pointer<Utf8> msgId);
typedef IntrovertUnelevateMessageDart = FfiResult Function(Pointer<Utf8> chatId, Pointer<Utf8> msgId);

typedef IntrovertGetElevatedMessagesC = FfiResult Function(Pointer<Utf8> chatId);
typedef IntrovertGetElevatedMessagesDart = FfiResult Function(Pointer<Utf8> chatId);

typedef IntrovertIsMessageElevatedC = FfiResult Function(Pointer<Utf8> chatId, Pointer<Utf8> msgId);
typedef IntrovertIsMessageElevatedDart = FfiResult Function(Pointer<Utf8> chatId, Pointer<Utf8> msgId);

typedef IntrovertStorageGetLastMessageC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertStorageGetLastMessageDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertStorageGetLastGroupMessageC = FfiResult Function(Pointer<Utf8> groupId);
typedef IntrovertStorageGetLastGroupMessageDart = FfiResult Function(Pointer<Utf8> groupId);

typedef IntrovertStorageGetLastMessagesAllC = FfiResult Function();
typedef IntrovertStorageGetLastMessagesAllDart = FfiResult Function();

typedef IntrovertStorageGetLastGroupMessagesAllC = FfiResult Function();
typedef IntrovertStorageGetLastGroupMessagesAllDart = FfiResult Function();

// Daily Rewards
typedef IntrovertDailyRewardGetStatusC = FfiResult Function();
typedef IntrovertDailyRewardGetStatusDart = FfiResult Function();

typedef IntrovertDailyRewardGetHistoryC = FfiResult Function(Uint32 days);
typedef IntrovertDailyRewardGetHistoryDart = FfiResult Function(int days);

typedef IntrovertDailyRewardRecordActivityC = FfiResult Function(Pointer<Uint8> jsonPtr, IntPtr jsonLen);
typedef IntrovertDailyRewardRecordActivityDart = FfiResult Function(Pointer<Uint8> jsonPtr, int jsonLen);

typedef IntrovertDailyRewardUpdateWeightsC = FfiResult Function(Pointer<Uint8> jsonPtr, IntPtr jsonLen);
typedef IntrovertDailyRewardUpdateWeightsDart = FfiResult Function(Pointer<Uint8> jsonPtr, int jsonLen);

typedef IntrovertDailyRewardUpdateAntiGamingC = FfiResult Function(Pointer<Uint8> jsonPtr, IntPtr jsonLen);
typedef IntrovertDailyRewardUpdateAntiGamingDart = FfiResult Function(Pointer<Uint8> jsonPtr, int jsonLen);

typedef IntrovertDailyRewardGetRealtimeEarningsC = FfiResult Function();
typedef IntrovertDailyRewardGetRealtimeEarningsDart = FfiResult Function();

// --- Event Models ---


class NetworkEvent implements Finalizable {
  final int type;
  final Uint8List data;
  NetworkEvent(this.type, this.data);
}

class MediaFrameEvent implements Finalizable {
  final int codec;
  final int width;
  final int height;
  final Pointer<Uint8> payload;
  final int payloadLen;
  final Pointer<Uint8> basePtr;
  final int baseLen;

  MediaFrameEvent({
    required this.codec,
    required this.width,
    required this.height,
    required this.payload,
    required this.payloadLen,
    required this.basePtr,
    required this.baseLen,
  });
}

class FileTransferProgress {
  final String transferId;
  final String peerId;
  final String filename;
  final String mimeType;
  final String fileHash;
  final double progress;
  final double speedBps;
  final bool isComplete;
  final bool isVerified;
  final bool isOutgoing;
  final bool isCancelled;
  final String? localPath;
  final int startTimeMs;
  final bool isWaitingForDownload;
  final String? thumbnail;
  final String? groupId;
  final String? caption;

  FileTransferProgress({
    required this.transferId,
    required this.peerId,
    required this.filename,
    required this.mimeType,
    required this.fileHash,
    required this.progress,
    required this.speedBps,
    required this.isComplete,
    required this.isVerified,
    required this.isOutgoing,
    required this.isCancelled,
    this.localPath,
    required this.startTimeMs,
    this.isWaitingForDownload = false,
    this.thumbnail,
    this.groupId,
    this.caption,
  });

  factory FileTransferProgress.fromJson(Map<String, dynamic> json) {
    return FileTransferProgress(
      transferId: json['transfer_id']?.toString() ?? '',
      peerId: json['peer_id']?.toString() ?? '',
      filename: json['filename']?.toString() ?? 'unknown',
      mimeType: json['mime_type']?.toString() ?? 'application/octet-stream',
      fileHash: json['file_hash']?.toString() ?? '',
      progress: (json['progress'] as num?)?.toDouble() ?? 0.0,
      speedBps: (json['speed_bps'] as num?)?.toDouble() ?? 0.0,
      isComplete: json['is_complete'] == true,
      isVerified: json['is_verified'] == true,
      isOutgoing: json['is_outgoing'] == true,
      isCancelled: json['is_cancelled'] == true,
      localPath: IntrovertClient().resolveSandboxPath(json['local_path']?.toString()),
      startTimeMs: (() { final v = (json['start_time_ms'] as num?)?.toInt(); return (v == null || v == 0) ? DateTime.now().millisecondsSinceEpoch : v; })(),
      isWaitingForDownload: false,
      thumbnail: json['thumbnail']?.toString(),
      groupId: json['group_id']?.toString(),
      caption: json['caption']?.toString(),
    );
  }

  DateTime get startDateTime => startTimeMs > 946684800000
      ? DateTime.fromMillisecondsSinceEpoch(startTimeMs)
      : DateTime.now();
}

// --- Main Client Implementation ---

typedef IntrovertStorageUpdateMessageStatusC = FfiResult Function(Pointer<Utf8> msgId, Uint8 status);
typedef IntrovertStorageUpdateMessageStatusDart = FfiResult Function(Pointer<Utf8> msgId, int status);

typedef IntrovertNetworkSendAcknowledgementC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msgId, Uint8 status);
typedef IntrovertNetworkSendAcknowledgementDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> msgId, int status);

typedef IntrovertStorageUpdateContactAliasC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> alias);
typedef IntrovertStorageUpdateContactAliasDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> alias);

typedef IntrovertNotesCreateC = FfiResult Function(Pointer<Utf8> id, Pointer<Utf8> title, Pointer<Utf8> content, Pointer<Utf8> tags, Pointer<Utf8> imagePath);
typedef IntrovertNotesCreateDart = FfiResult Function(Pointer<Utf8> id, Pointer<Utf8> title, Pointer<Utf8> content, Pointer<Utf8> tags, Pointer<Utf8> imagePath);
typedef IntrovertNotesUpdateC = FfiResult Function(Pointer<Utf8> id, Pointer<Utf8> title, Pointer<Utf8> content, Pointer<Utf8> tags, Pointer<Utf8> imagePath);
typedef IntrovertNotesUpdateDart = FfiResult Function(Pointer<Utf8> id, Pointer<Utf8> title, Pointer<Utf8> content, Pointer<Utf8> tags, Pointer<Utf8> imagePath);
typedef IntrovertNotesDeleteC = FfiResult Function(Pointer<Utf8> id);
typedef IntrovertNotesDeleteDart = FfiResult Function(Pointer<Utf8> id);
typedef IntrovertNotesGetC = FfiResult Function(Pointer<Utf8> id);
typedef IntrovertNotesGetDart = FfiResult Function(Pointer<Utf8> id);
typedef IntrovertNotesGetAllC = FfiResult Function();
typedef IntrovertNotesGetAllDart = FfiResult Function();
typedef IntrovertNotesSearchC = FfiResult Function(Pointer<Utf8> query);
typedef IntrovertNotesSearchDart = FfiResult Function(Pointer<Utf8> query);
typedef IntrovertNotesSaveVersionC = FfiResult Function(Pointer<Utf8> noteId, Pointer<Utf8> title, Pointer<Utf8> content, Pointer<Utf8> tags);
typedef IntrovertNotesSaveVersionDart = FfiResult Function(Pointer<Utf8> noteId, Pointer<Utf8> title, Pointer<Utf8> content, Pointer<Utf8> tags);
typedef IntrovertNotesGetVersionsC = FfiResult Function(Pointer<Utf8> noteId);
typedef IntrovertNotesGetVersionsDart = FfiResult Function(Pointer<Utf8> noteId);

class IntrovertClient {
  static final IntrovertClient _instance = IntrovertClient._internal();
  factory IntrovertClient() => _instance;

  late DynamicLibrary _dylib;
  NativeFinalizer? _binaryFinalizer;
  
  late IntrovertGenerateMnemonicDart _generateMnemonic;
  late IntrovertFreeStringDart _freeString;
  late IntrovertFreeBinaryDart _freeBinary;
  late IntrovertMnemonicToSeedDart _mnemonicToSeed;
  late IntrovertDeriveIdentifiersDart _deriveIdentifiers;
  late IntrovertEngineStartDart _engineStart;
  late IntrovertEngineStopDart _engineStop;
  late IntrovertGetPeerIdDart _getPeerId;
  late IntrovertNetworkStartDart _networkStart;
  late IntrovertEconomyStartMonitoringDart _economyStartMonitoring;
  late IntrovertNetworkSendMessageDart _networkSendMessage;
  late IntrovertNetworkInitiateWebRtcDart _networkInitiateWebRtc;
  late IntrovertWebRtcSendNativeSignalDart _sendNativeSignal;
  late IntrovertAddAddressDart _addAddress;
  late IntrovertClaimRewardsAsyncDart _claimRewardsAsync;
  late IntrovertStoreMessageAsyncDart _storeMessageAsync;
  late IntrovertStorageGetMessagesDart _getMessages;
  late IntrovertStorageGetMessagesPaginatedDart _getMessagesPaginated;
  late IntrovertEstablishSecureSessionDart _establishSecureSession;
  late IntrovertFetchMailboxDart _fetchMailbox;
  late IntrovertStartMediaStreamDart _startMediaStream;
  late IntrovertStorageGetContactsDart _getContacts;
  late IntrovertDeleteContactDart _deleteContact;
  late IntrovertSetProfileTierDart _setProfileTier;
  late IntrovertDeleteChatDart _deleteChat;
  late IntrovertClearContactsDart _clearContacts;
  late IntrovertWormholeStartDart _wormholeStart;
  late IntrovertWormholeJoinDart _wormholeJoin;
  late IntrovertWormholeAbortDart _wormholeAbort;
  late IntrovertCloseWebRtcDart _closeWebRtc;
  late IntrovertRenegotiateWebRtcDart _renegotiateWebRtc;
  late IntrovertAcceptCallDart _acceptCall;
  late IntrovertRejectCallDart _rejectCall;
  late IntrovertSetAnchorModeDart _setAnchorMode;
  late IntrovertGetAnchorModeDart _getAnchorMode;
  late IntrovertNetworkSetConnectivityTypeDart _setConnectivityType;
  late IntrovertNetworkSetTunnelModeDart _setTunnelMode;
  late IntrovertNetworkGetTunnelModeDart _getTunnelMode;
  late IntrovertNetworkGetRbnsDart _getRbns;
  late IntrovertNetworkTestRbnDart _testRbn;
  late IntrovertDisclaimerIsAcceptedDart _disclaimerIsAccepted;
  late IntrovertDisclaimerSetAcceptedDart _disclaimerSetAccepted;
  late IntrovertNetworkRecheckConnectionDart _recheckConnection;
  late IntrovertNetworkResolveHandleDart _resolveHandle;
  late IntrovertNetworkSendDirectInviteDart _sendDirectInvite;
  late IntrovertNetworkRegisterPushTokenDart _registerPushToken;
  late IntrovertNetworkSetRetentionDart _setRetention;
  late IntrovertNetworkDeleteMessageDart _deleteMessage;
  late IntrovertNetworkEditMessageDart _editMessage;
  late IntrovertNetworkSendReactionDart _sendReaction;
  late IntrovertStorageGetReactionsDart _getReactions;
  late IntrovertNetworkClaimHandleDart _claimHandle;
  late IntrovertStorageGetHandleStatusDart _getHandleStatus;
  late IntrovertStorageGetLocalHandleDart _getLocalHandle;
  late IntrovertStorageIsHandleClaimedDart _isHandleClaimed;
  late IntrovertNetworkRequestSwarmStatsDart _requestSwarmStats;
  late IntrovertNetworkPollPeerProfileDart _pollPeerProfile;
  late IntrovertNetworkSyncChatMessagesDart _syncChatMessages;
  late IntrovertStorageGetProfileDart _getProfile;
  late IntrovertStorageSetProfileDart _setProfile;
  late IntrovertNetworkSendFileDart _sendFile;
  late IntrovertNetworkCancelFileTransferDart _cancelFileTransfer;
  late IntrovertNetworkForceRefreshDart _forceNetworkRefresh;
  late IntrovertSendManualTelemetryDart _sendManualTelemetry;
  late IntrovertGroupCreateDart _groupCreate;
  late IntrovertGroupSendMessageDart _groupSendMessage;
  late IntrovertGroupGetAllDart _groupGetAll;
  late IntrovertGroupGetMessagesDart _groupGetMessages;
  late IntrovertGroupAddMemberDart _groupAddMember;
  late IntrovertGroupApproveJoinDart _groupApproveJoin;
  late IntrovertGroupRejectJoinDart _groupRejectJoin;
  late IntrovertGroupRemoveMemberDart _groupRemoveMember;
  late IntrovertGroupUpdateRoleDart _groupUpdateRole;
  late IntrovertGroupPublishManifestDart _groupPublishManifest;
  late IntrovertGroupJoinByCodeDart _groupJoinByCode;
  late IntrovertGroupDeleteDart _groupDelete;
  late IntrovertGroupGetPendingInvitesDart _groupGetPendingInvites;
  late IntrovertGroupAcceptInviteDart _groupAcceptInvite;
  late IntrovertGroupDeclineInviteDart _groupDeclineInvite;
  late IntrovertGroupMuteMemberDart _groupMuteMember;
  late IntrovertGroupUnmuteMemberDart _groupUnmuteMember;
  late IntrovertGroupGetMutedMembersDart _groupGetMutedMembers;
  late IntrovertStorageUpdateMessageStatusDart _updateMessageStatus;
  late IntrovertStorageGetUnreadCountsDart _getUnreadCounts;
  late IntrovertStorageUpdateGroupMessageStatusDart _updateGroupMessageStatus;
  late IntrovertStorageUpdateGroupMessageStatusByIdDart _updateGroupMessageStatusById;
  late IntrovertStorageUpdateMessageStatusForPeerDart _updateMessageStatusForPeer;
  late IntrovertNetworkSendAcknowledgementDart _sendAcknowledgement;
  late IntrovertStorageUpdateContactAliasDart _updateContactAlias;
  late IntrovertNetworkComputeFileHashDart _computeFileHash;
  late IntrovertNukeIdentityDart _nukeIdentity;
  late IntrovertDriveAddFileDart _driveAddFile;
  late IntrovertDriveGetAllDart _driveGetAll;
  late IntrovertDriveGetByHashDart _driveGetByHash;
  late IntrovertDriveDeleteDart _driveDelete;
  late IntrovertDriveAddFileWithFolderDart _driveAddFileWithFolder;
  late IntrovertDriveUpdateFolderDart _driveUpdateFolder;
  late IntrovertGetMeshCapacityDart _getMeshCapacity;
  late IntrovertGetDiskSpaceDart _getDiskSpace;
  late IntrovertNetworkRegisterSeederDart _registerSeeder;
  late IntrovertNetworkStartPullDart _startPull;
  late IntrovertNotesCreateDart _notesCreate;
  late IntrovertNotesUpdateDart _notesUpdate;
  late IntrovertNotesDeleteDart _notesDelete;
  late IntrovertNotesGetDart _notesGet;
  late IntrovertNotesGetAllDart _notesGetAll;
  late IntrovertNotesSearchDart _notesSearch;
  late IntrovertNotesSaveVersionDart _notesSaveVersion;
  late IntrovertNotesGetVersionsDart _notesGetVersions;

  // --- Intro-Claw AI Engine Mode ---
  late IntroClawGetAiModeDart _getAiMode;
  late IntroClawSetAiModeDart _setAiMode;
  late IntroClawGetApiKeyDart _getApiKey;
  late IntroClawTriggerTickDart _clawTriggerTick;
  late IntroClawSetActiveDart _clawSetActive;
  late IntroClawSetNodeModeDart _clawSetNodeMode;
  late IntroClawGetStatusDart _clawGetStatus;
  late IntroClawGetEndpointDart _clawGetEndpoint;
  late IntroClawSetEndpointDart _clawSetEndpoint;
  late IntroClawProcessQueryDart _clawProcessQuery;
  late IntroClawRunNetworkReconDart _clawRunNetworkRecon;
  late IntroClawHealPeerDart _clawHealPeer;
  late IntroClawGetActivityLogDart _clawGetActivityLog;
  late IntroClawVoipStartCallDart _clawVoipStartCall;
  late IntroClawVoipEndCallDart _clawVoipEndCall;
  late IntroClawVoipRecordSampleDart _clawVoipRecordSample;
  late IntroClawVoipGetQualityDart _clawVoipGetQuality;
  late IntroClawVoipGetDowngradeRecommendationDart _clawVoipGetDowngradeRecommendation;
  late IntroClawSetActiveChatDart _clawSetActiveChat;
  late IntroClawClearActiveChatDart _clawClearActiveChat;
  late IntroClawSetActiveGroupMembersDart _clawSetActiveGroupMembers;
  late IntroClawOnAppLaunchDart _clawOnAppLaunch;

  // Elevated Messages
  late IntrovertElevateMessageDart _elevateMessage;
  late IntrovertUnelevateMessageDart _unelevateMessage;
  late IntrovertGetElevatedMessagesDart _getElevatedMessages;
  late IntrovertIsMessageElevatedDart _isMessageElevated;

  // Optimized last message queries
  late IntrovertStorageGetLastMessageDart _getLastMessage;
  late IntrovertStorageGetLastGroupMessageDart _getLastGroupMessage;
  late IntrovertStorageGetLastMessagesAllDart _getLastMessagesAll;
  late IntrovertStorageGetLastGroupMessagesAllDart _getLastGroupMessagesAll;

  // Daily Rewards
  late IntrovertDailyRewardGetStatusDart _dailyRewardGetStatus;
  late IntrovertDailyRewardGetHistoryDart _dailyRewardGetHistory;
  late IntrovertDailyRewardRecordActivityDart _dailyRewardRecordActivity;
  late IntrovertDailyRewardUpdateWeightsDart _dailyRewardUpdateWeights;
  late IntrovertDailyRewardUpdateAntiGamingDart _dailyRewardUpdateAntiGaming;
  late IntrovertDailyRewardGetRealtimeEarningsDart _dailyRewardGetRealtimeEarnings;
  GetRewardsStateDart? _getRewardsState;

  NativeCallable<NativeNetworkCallback>? _unifiedCallable;

  int? _lastLocalStatus;
  int? get localStatus => _lastLocalStatus;

  final StreamController<NetworkEvent> _networkStreamController = StreamController<NetworkEvent>.broadcast();
  Stream<NetworkEvent> get networkStream => _networkStreamController.stream;

  final StreamController<MediaFrameEvent> _mediaStreamController = StreamController<MediaFrameEvent>.broadcast();
  Stream<MediaFrameEvent> get mediaStream => _mediaStreamController.stream;

  final StreamController<FileTransferProgress> _transferStreamController = StreamController<FileTransferProgress>.broadcast();
  Stream<FileTransferProgress> get transferStream => _transferStreamController.stream;

  final StreamController<Map<String, dynamic>> _economyStreamController = StreamController<Map<String, dynamic>>.broadcast();
  Stream<Map<String, dynamic>> get economyStream => _economyStreamController.stream;

  final StreamController<Map<String, dynamic>> _swarmStatsStreamController = StreamController<Map<String, dynamic>>.broadcast();
  Stream<Map<String, dynamic>> get swarmStatsStream => _swarmStatsStreamController.stream;

  final StreamController<Map<String, dynamic>> _telemetryAckStreamController = StreamController<Map<String, dynamic>>.broadcast();
  Stream<Map<String, dynamic>> get telemetryAckStream => _telemetryAckStreamController.stream;

  // --- In-App Rust Debug Log Ring Buffer ---
  // Captures event-99 (Rust debug) messages so they can be saved/copied on-device
  // without needing a USB-connected debugger. Holds the last 500 entries.
  static const int _maxDebugLogEntries = 500;
  final List<String> _rustDebugLogs = [];

  /// Returns all captured Rust debug log entries as a single formatted string.
  String getDebugLogs() {
    if (_rustDebugLogs.isEmpty) return '(no debug logs captured)';
    return _rustDebugLogs.join('\n');
  }

  /// Returns the number of Rust debug log entries captured.
  int get debugLogCount => _rustDebugLogs.length;

  /// Clears all captured Rust debug log entries.
  void clearDebugLogs() => _rustDebugLogs.clear();

  String? _supportDirPath;
  String? _documentsDirPath;

  void initSandboxPaths(String supportPath, String docsPath) {
    _supportDirPath = supportPath;
    _documentsDirPath = docsPath;
    if (kDebugMode) debugPrint("📂 Sandbox Paths Initialized: Support='$_supportDirPath', Docs='$_documentsDirPath'");
  }

  String? resolveSandboxPath(String? path) {
    if (path == null || path.isEmpty) return null;
    
    // Normalize path separators
    final normalizedPath = path.replaceAll('\\', '/');
    
    // Security: reject obvious traversal attempts
    if (normalizedPath.contains('/../') || normalizedPath.contains('\\..\\') || normalizedPath.startsWith('../')) {
      return null;
    }
    
    // Check if the path contains 'Library/Application Support/' or 'Documents/'
    const supportPattern = 'Library/Application Support/';
    const docsPattern = 'Documents/';
    
    if (normalizedPath.contains(supportPattern)) {
      final index = normalizedPath.indexOf(supportPattern) + supportPattern.length;
      var relativePart = normalizedPath.substring(index);
      if (_supportDirPath != null) {
        final normSupport = _supportDirPath!.replaceAll('\\', '/');
        final supportIdx = normSupport.indexOf(supportPattern);
        if (supportIdx != -1) {
          final supportSuffix = normSupport.substring(supportIdx + supportPattern.length);
          if (supportSuffix.isNotEmpty) {
            if (relativePart.startsWith('$supportSuffix/')) {
              relativePart = relativePart.substring(supportSuffix.length + 1);
            } else if (relativePart == supportSuffix) {
              relativePart = '';
            }
          }
        }
        final resolved = '${normSupport.replaceAll(RegExp(r'/+$'), '')}/${relativePart.replaceAll(RegExp(r'^/+'), '')}';
        // Security: verify resolved path stays within sandbox
        if (_supportDirPath != null && !resolved.startsWith(_supportDirPath!)) {
          return null;
        }
        return resolved;
      }
    } else if (normalizedPath.contains(docsPattern)) {
      final index = normalizedPath.indexOf(docsPattern) + docsPattern.length;
      var relativePart = normalizedPath.substring(index);
      if (_documentsDirPath != null) {
        final normDocs = _documentsDirPath!.replaceAll('\\', '/');
        final docsIdx = normDocs.indexOf(docsPattern);
        if (docsIdx != -1) {
          final docsSuffix = normDocs.substring(docsIdx + docsPattern.length);
          if (docsSuffix.isNotEmpty) {
            if (relativePart.startsWith('$docsSuffix/')) {
              relativePart = relativePart.substring(docsSuffix.length + 1);
            } else if (relativePart == docsSuffix) {
              relativePart = '';
            }
          }
        }
        final resolved = '${normDocs.replaceAll(RegExp(r'/+$'), '')}/${relativePart.replaceAll(RegExp(r'^/+'), '')}';
        // Security: verify resolved path stays within sandbox
        if (_documentsDirPath != null && !resolved.startsWith(_documentsDirPath!)) {
          return null;
        }
        return resolved;
      }
    }
    
    return path;
  }

  IntrovertClient._internal() {
    _loadLibrary();
    _bindFunctions();
    _initializeFinalizer();
    _initializeUnifiedCallable();
  }

  void _initializeUnifiedCallable() {
    if (_unifiedCallable != null) return;
    _unifiedCallable = NativeCallable<NativeNetworkCallback>.listener((int eventType, Pointer<Uint8> dataPtr, int dataLen) {
      if (dataPtr.address == 0) return;
      
      final Pointer<Uint8> castedPtr = dataPtr.cast<Uint8>();
      
      if (eventType == 5) { // Media Frame
        final header = castedPtr.cast<MediaFrameHeader>().ref;
        final headerSize = sizeOf<MediaFrameHeader>();
        final event = MediaFrameEvent(
          codec: header.codec,
          width: header.width,
          height: header.height,
          payload: Pointer<Uint8>.fromAddress(castedPtr.address + headerSize),
          payloadLen: dataLen - headerSize,
          basePtr: castedPtr,
          baseLen: dataLen,
        );
        _binaryFinalizer?.attach(event, castedPtr.cast<Void>(), externalSize: dataLen);
        if (!_mediaStreamController.isClosed) _mediaStreamController.add(event);
      } else if (eventType == 9) { // Economy Stats
        try {
          final data = castedPtr.asTypedList(dataLen);
          final stats = json.decode(utf8.decode(data)) as Map<String, dynamic>;
          if (!stats.containsKey('sol_balance')) {
            stats['sol_balance'] = stats['intr_balance'] ?? 0;
          }
          if (!_economyStreamController.isClosed) _economyStreamController.add(stats);
        } catch (e) {
          debugPrint("❌ Error decoding economy stats: $e");
        } finally {
          _freeBinary(dataPtr, dataLen);
        }
      } else if (eventType == 12) { // File Transfer Progress
        try {
          final data = castedPtr.asTypedList(dataLen);
          final jsonStr = utf8.decode(data);
          final progress = FileTransferProgress.fromJson(json.decode(jsonStr));
          if (!_transferStreamController.isClosed) _transferStreamController.add(progress);
        } catch (e) {
          debugPrint("❌ Error decoding file progress: $e");
        } finally {
          _freeBinary(dataPtr, dataLen);
        }
      } else if (eventType == 30) { // Swarm Stats
        try {
          final data = castedPtr.asTypedList(dataLen);
          final jsonStr = utf8.decode(data);
          if (!_swarmStatsStreamController.isClosed) _swarmStatsStreamController.add(json.decode(jsonStr) as Map<String, dynamic>);
        } catch (e) {
          debugPrint("❌ Error decoding swarm stats: $e");
        } finally {
          _freeBinary(dataPtr, dataLen);
        }
      } else if (eventType == 41) { // Telemetry Acknowledgment (separate from Event 40 = Message Reactions)
        try {
          final data = castedPtr.asTypedList(dataLen);
          final jsonStr = utf8.decode(data);
          if (!_telemetryAckStreamController.isClosed) _telemetryAckStreamController.add(json.decode(jsonStr) as Map<String, dynamic>);
        } catch (e) {
          debugPrint("❌ Error decoding telemetry ack: $e");
        } finally {
          _freeBinary(dataPtr, dataLen);
        }
      } else if (eventType == 99) { // Rust Debug
        final data = castedPtr.asTypedList(dataLen);
        final msg = utf8.decode(data);
        debugPrint('🦀 Rust Debug: $msg');
        // Store in ring buffer for on-device capture
        final ts = DateTime.now().toIso8601String();
        final entry = '[$ts] $msg';
        if (_rustDebugLogs.length >= _maxDebugLogEntries) {
          _rustDebugLogs.removeAt(0); // Drop oldest
        }
        _rustDebugLogs.add(entry);
        _freeBinary(dataPtr, dataLen);
      } else {
        // All other events (2, 4, 7, 8, 10, 11, 13, etc.) go to network stream
        if (eventType == 10 && dataLen > 0) {
          _lastLocalStatus = castedPtr.asTypedList(dataLen)[0];
        }
        final data = castedPtr.asTypedList(dataLen);
        final eventData = Uint8List.fromList(data);
        final event = NetworkEvent(eventType, eventData);
        
        // We copy the data into eventData and then free the native buffer immediately.
        if (!_networkStreamController.isClosed) {
          _networkStreamController.add(event);
        }
        _freeBinary(dataPtr, dataLen);
      }
    });
  }

  void _initializeFinalizer() {
    try {
      final freeFunc = _dylib.lookup<NativeFunction<Void Function(Pointer<Void>)>>('introvert_free_binary_finalizer');
      _binaryFinalizer = NativeFinalizer(freeFunc.cast());
      debugPrint('✅ Native finalizer initialized.');
    } catch (e) {
      debugPrint('ℹ️ Native finalizer NOT available: $e');
      _binaryFinalizer = null;
    }
  }

  void _loadLibrary() {
    if (Platform.isAndroid || Platform.isLinux) {
      try {
        _dylib = DynamicLibrary.open('libintrovert.so');
        debugPrint('✅ Loaded libintrovert.so');
      } catch (e) {
        debugPrint('❌ Failed to load libintrovert.so: $e');
        rethrow;
      }
    } else if (Platform.isMacOS) {
      // App bundle path: .app/Contents/Frameworks/libintrovert.dylib
      final exeDir = File(Platform.resolvedExecutable).parent; // Contents/MacOS/
      final bundleFrameworks = '${exeDir.parent.path}/Frameworks/libintrovert.dylib';
      final List<String> possiblePaths = [
        bundleFrameworks,
        '${Directory.current.path}/libintrovert.dylib',
        '${Directory.current.path}/macos/Flutter/ephemeral/libintrovert.dylib',
        'libintrovert.dylib',
      ];

      debugPrint('ℹ️ macOS: Searching for libintrovert.dylib...');
      for (final path in possiblePaths) {
        try {
          if (path.startsWith('/') && !File(path).existsSync()) continue;
          _dylib = DynamicLibrary.open(path);
          debugPrint('✅ Loaded native library: $path');
          return;
        } catch (e) {
          // Continue
        }
      }

      try {
        _dylib = DynamicLibrary.process();
        debugPrint('ℹ️ Falling back to process-level lookup');
      } catch (e) {
        throw UnsupportedError('Could not load libintrovert.dylib: $e');
      }
    } else if (Platform.isIOS) {
      _dylib = DynamicLibrary.process();
    } else {
      throw UnsupportedError('Unsupported platform.');
    }
  }

  void _bindFunctions() {
    T safeLookup<T>(String name, T Function() lookup, T fallback) {
      try {
        final result = lookup();
        debugPrint('✅ FFI Bound: $name');
        return result;
      } catch (e) {
        debugPrint('⚠️ FFI Bind FAILED: $name (Using placeholder)');
        return fallback;
      }
    }

    try {
      _generateMnemonic = safeLookup('generate_mnemonic', () => _dylib.lookupFunction<IntrovertGenerateMnemonicC, IntrovertGenerateMnemonicDart>('introvert_generate_mnemonic'), () => nullptr);
      _freeString = safeLookup('free_string', () => _dylib.lookupFunction<IntrovertFreeStringC, IntrovertFreeStringDart>('introvert_free_string'), (ptr) {});
      _freeBinary = safeLookup('free_binary', () => _dylib.lookupFunction<IntrovertFreeBinaryC, IntrovertFreeBinaryDart>('introvert_free_binary'), (ptr, len) {});
      _mnemonicToSeed = safeLookup('mnemonic_to_seed', () => _dylib.lookupFunction<IntrovertMnemonicToSeedC, IntrovertMnemonicToSeedDart>('introvert_mnemonic_to_seed'), (ptr) => FfiResult.dummy);
      _deriveIdentifiers = safeLookup('derive_identifiers', () => _dylib.lookupFunction<IntrovertDeriveIdentifiersC, IntrovertDeriveIdentifiersDart>('introvert_derive_identifiers'), (s) => FfiResult.dummy);
      _engineStart = safeLookup('engine_start', () => _dylib.lookupFunction<IntrovertEngineStartC, IntrovertEngineStartDart>('introvert_engine_start'), (s, p) => FfiResult.dummy);
      _engineStop = safeLookup('engine_stop', () => _dylib.lookupFunction<IntrovertEngineStopC, IntrovertEngineStopDart>('introvert_engine_stop'), () => FfiResult.dummy);
      _getPeerId = safeLookup('get_peer_id', () => _dylib.lookupFunction<IntrovertGetPeerIdC, IntrovertGetPeerIdDart>('introvert_get_peer_id'), () => nullptr);
      _networkStart = safeLookup('network_start', () => _dylib.lookupFunction<IntrovertNetworkStartC, IntrovertNetworkStartDart>('introvert_network_start_production'), (cb, p, r, m, l) => FfiResult.dummy);
      _economyStartMonitoring = safeLookup('economy_monitor', () => _dylib.lookupFunction<IntrovertEconomyStartMonitoringC, IntrovertEconomyStartMonitoringDart>('introvert_economy_start_monitoring'), (cb) => FfiResult.dummy);
      _networkSendMessage = safeLookup('send_message', () => _dylib.lookupFunction<IntrovertNetworkSendMessageC, IntrovertNetworkSendMessageDart>('introvert_network_send_message'), (p, m, r, cb) => FfiResult.dummy);
      _networkInitiateWebRtc = safeLookup('init_webrtc', () => _dylib.lookupFunction<IntrovertNetworkInitiateWebRtcC, IntrovertNetworkInitiateWebRtcDart>('introvert_network_initiate_webrtc'), (p, m, cb) => FfiResult.dummy);
      _sendNativeSignal = safeLookup('send_native_signal', () => _dylib.lookupFunction<IntrovertWebRtcSendNativeSignalC, IntrovertWebRtcSendNativeSignalDart>('introvert_webrtc_send_native_signal'), (p, j) => FfiResult.dummy);
      _addAddress = safeLookup('add_address', () => _dylib.lookupFunction<IntrovertAddAddressC, IntrovertAddAddressDart>('introvert_network_add_address'), (p, a) => FfiResult.dummy);
      _claimRewardsAsync = safeLookup('claim_rewards', () => _dylib.lookupFunction<IntrovertClaimRewardsAsyncC, IntrovertClaimRewardsAsyncDart>('introvert_claim_rewards_async'), (cb) => FfiResult.dummy);
      _storeMessageAsync = safeLookup('store_msg_async', () => _dylib.lookupFunction<IntrovertStoreMessageAsyncC, IntrovertStoreMessageAsyncDart>('introvert_store_message_async'), (p, m, me, cb) => FfiResult.dummy);
      _getMessages = safeLookup('get_messages', () => _dylib.lookupFunction<IntrovertStorageGetMessagesC, IntrovertStorageGetMessagesDart>('introvert_storage_get_messages'), (p) => FfiResult.dummy);
      _getMessagesPaginated = safeLookup('get_messages_paginated', () => _dylib.lookupFunction<IntrovertStorageGetMessagesPaginatedC, IntrovertStorageGetMessagesPaginatedDart>('introvert_storage_get_messages_paginated'), (p, o, l) => FfiResult.dummy);
      _establishSecureSession = safeLookup('secure_session', () => _dylib.lookupFunction<IntrovertEstablishSecureSessionC, IntrovertEstablishSecureSessionDart>('introvert_network_establish_secure_session'), (p) => FfiResult.dummy);
      _fetchMailbox = safeLookup('fetch_mailbox', () => _dylib.lookupFunction<IntrovertFetchMailboxC, IntrovertFetchMailboxDart>('introvert_network_fetch_mailbox'), () => FfiResult.dummy);
      _startMediaStream = safeLookup('media_stream', () => _dylib.lookupFunction<IntrovertStartMediaStreamC, IntrovertStartMediaStreamDart>('introvert_network_start_media_stream'), (p, t) => FfiResult.dummy);
      _getContacts = safeLookup('get_contacts', () => _dylib.lookupFunction<IntrovertStorageGetContactsC, IntrovertStorageGetContactsDart>('introvert_storage_get_contacts'), () => FfiResult.dummy);
      _deleteContact = safeLookup('delete_contact', () => _dylib.lookupFunction<IntrovertDeleteContactC, IntrovertDeleteContactDart>('introvert_storage_delete_contact'), (p) => FfiResult.dummy);
      _setProfileTier = safeLookup('set_profile_tier', () => _dylib.lookupFunction<IntrovertSetProfileTierC, IntrovertSetProfileTierDart>('introvert_storage_set_profile_tier'), (t) => FfiResult.dummy);
      _deleteChat = safeLookup('delete_chat', () => _dylib.lookupFunction<IntrovertDeleteChatC, IntrovertDeleteChatDart>('introvert_storage_delete_chat'), (p) => FfiResult.dummy);
      _clearContacts = safeLookup('clear_contacts', () => _dylib.lookupFunction<IntrovertClearContactsC, IntrovertClearContactsDart>('introvert_storage_clear_contacts'), () => FfiResult.dummy);
      _wormholeStart = safeLookup('wormhole_start', () => _dylib.lookupFunction<IntrovertWormholeStartC, IntrovertWormholeStartDart>('introvert_wormhole_start'), () => FfiResult.dummy);
      _wormholeJoin = safeLookup('wormhole_join', () => _dylib.lookupFunction<IntrovertWormholeJoinC, IntrovertWormholeJoinDart>('introvert_wormhole_join'), (c) => FfiResult.dummy);
      _wormholeAbort = safeLookup('wormhole_abort', () => _dylib.lookupFunction<IntrovertWormholeAbortC, IntrovertWormholeAbortDart>('introvert_wormhole_abort'), () => FfiResult.dummy);
      _closeWebRtc = safeLookup('close_webrtc', () => _dylib.lookupFunction<IntrovertCloseWebRtcC, IntrovertCloseWebRtcDart>('introvert_webrtc_close_connection'), (p) => FfiResult.dummy);
      _renegotiateWebRtc = safeLookup('renegotiate_webrtc', () => _dylib.lookupFunction<IntrovertRenegotiateWebRtcC, IntrovertRenegotiateWebRtcDart>('introvert_webrtc_renegotiate'), (p) => FfiResult.dummy);
      _acceptCall = safeLookup('accept_call', () => _dylib.lookupFunction<IntrovertAcceptCallC, IntrovertAcceptCallDart>('introvert_network_accept_call'), (p, m) => FfiResult.dummy);
      _rejectCall = safeLookup('reject_call', () => _dylib.lookupFunction<IntrovertRejectCallC, IntrovertRejectCallDart>('introvert_network_reject_call'), (p) => FfiResult.dummy);
      _setAnchorMode = safeLookup('set_anchor', () => _dylib.lookupFunction<IntrovertSetAnchorModeC, IntrovertSetAnchorModeDart>('introvert_network_set_anchor_mode'), (e) => FfiResult.dummy);
      _getAnchorMode = safeLookup('get_anchor_mode', () => _dylib.lookupFunction<IntrovertGetAnchorModeC, IntrovertGetAnchorModeDart>('introvert_network_get_anchor_mode'), () => 0);
      _setTunnelMode = safeLookup('set_tunnel', () => _dylib.lookupFunction<IntrovertNetworkSetTunnelModeC, IntrovertNetworkSetTunnelModeDart>('introvert_network_set_tunnel_mode'), (e) => FfiResult.dummy);
      _getTunnelMode = safeLookup('get_tunnel_mode', () => _dylib.lookupFunction<IntrovertNetworkGetTunnelModeC, IntrovertNetworkGetTunnelModeDart>('introvert_network_get_tunnel_mode'), () => 0);
      _getRbns = safeLookup('get_rbns', () => _dylib.lookupFunction<IntrovertNetworkGetRbnsC, IntrovertNetworkGetRbnsDart>('introvert_network_get_rbns'), () => FfiResult.dummy);
      _testRbn = safeLookup('test_rbn', () => _dylib.lookupFunction<IntrovertNetworkTestRbnC, IntrovertNetworkTestRbnDart>('introvert_network_test_rbn'), (addr) => FfiResult.dummy);
      _disclaimerIsAccepted = safeLookup('disclaimer_is_accepted', () => _dylib.lookupFunction<IntrovertDisclaimerIsAcceptedC, IntrovertDisclaimerIsAcceptedDart>('introvert_disclaimer_is_accepted'), (p, s) => 0);
      _disclaimerSetAccepted = safeLookup('disclaimer_set_accepted', () => _dylib.lookupFunction<IntrovertDisclaimerSetAcceptedC, IntrovertDisclaimerSetAcceptedDart>('introvert_disclaimer_set_accepted'), (p, s, a) => FfiResult.dummy);
      _recheckConnection = safeLookup('recheck_connection', () => _dylib.lookupFunction<IntrovertNetworkRecheckConnectionC, IntrovertNetworkRecheckConnectionDart>('introvert_network_recheck_connection'), (p) => FfiResult.dummy);
      _resolveHandle = safeLookup('resolve_handle', () => _dylib.lookupFunction<IntrovertNetworkResolveHandleC, IntrovertNetworkResolveHandleDart>('introvert_network_resolve_handle'), (h) => FfiResult.dummy);
      _sendDirectInvite = safeLookup('send_direct_invite', () => _dylib.lookupFunction<IntrovertNetworkSendDirectInviteC, IntrovertNetworkSendDirectInviteDart>('introvert_network_send_direct_invite'), (p) => FfiResult.dummy);
      _registerPushToken = safeLookup('register_push_token', () => _dylib.lookupFunction<IntrovertNetworkRegisterPushTokenC, IntrovertNetworkRegisterPushTokenDart>('introvert_network_register_push_token'), (d, t) => FfiResult.dummy);
      _setRetention = safeLookup('set_retention', () => _dylib.lookupFunction<IntrovertNetworkSetRetentionC, IntrovertNetworkSetRetentionDart>('introvert_network_set_retention'), (t, h, g) => FfiResult.dummy);
      _deleteMessage = safeLookup('delete_message', () => _dylib.lookupFunction<IntrovertNetworkDeleteMessageC, IntrovertNetworkDeleteMessageDart>('introvert_network_delete_message'), (t, m, g, a) => FfiResult.dummy);
      _editMessage = safeLookup('edit_message', () => _dylib.lookupFunction<IntrovertNetworkEditMessageC, IntrovertNetworkEditMessageDart>('introvert_network_edit_message'), (t, m, n, g) => FfiResult.dummy);
      _sendReaction = safeLookup('send_reaction', () => _dylib.lookupFunction<IntrovertNetworkSendReactionC, IntrovertNetworkSendReactionDart>('introvert_network_send_reaction'), (t, m, e, g) => FfiResult.dummy);
      _getReactions = safeLookup('get_reactions', () => _dylib.lookupFunction<IntrovertStorageGetReactionsC, IntrovertStorageGetReactionsDart>('introvert_storage_get_reactions'), (m) => FfiResult.dummy);
      _claimHandle = safeLookup('claim_handle', () => _dylib.lookupFunction<IntrovertNetworkClaimHandleC, IntrovertNetworkClaimHandleDart>('introvert_network_claim_handle'), (h) => FfiResult.dummy);
      _getHandleStatus = safeLookup('get_handle_status', () => _dylib.lookupFunction<IntrovertStorageGetHandleStatusC, IntrovertStorageGetHandleStatusDart>('introvert_storage_get_handle_status'), (h) => FfiResult.dummy);
      _getLocalHandle = safeLookup('get_local_handle', () => _dylib.lookupFunction<IntrovertStorageGetLocalHandleC, IntrovertStorageGetLocalHandleDart>('introvert_storage_get_local_handle'), () => FfiResult.dummy);
      _isHandleClaimed = safeLookup('is_handle_claimed', () => _dylib.lookupFunction<IntrovertStorageIsHandleClaimedC, IntrovertStorageIsHandleClaimedDart>('introvert_storage_is_handle_claimed'), (h) => FfiResult.dummy);
      _requestSwarmStats = safeLookup('request_swarm_stats', () => _dylib.lookupFunction<IntrovertNetworkRequestSwarmStatsC, IntrovertNetworkRequestSwarmStatsDart>('introvert_network_request_swarm_stats'), () => FfiResult.dummy);
      _pollPeerProfile = safeLookup('poll_peer_profile', () => _dylib.lookupFunction<IntrovertNetworkPollPeerProfileC, IntrovertNetworkPollPeerProfileDart>('introvert_network_poll_peer_profile'), (p) => FfiResult.dummy);
      _syncChatMessages = safeLookup('sync_chat_messages', () => _dylib.lookupFunction<IntrovertNetworkSyncChatMessagesC, IntrovertNetworkSyncChatMessagesDart>('introvert_network_sync_chat_messages'), (p, a, b, c) => FfiResult.dummy);
      _getProfile = safeLookup('get_profile', () => _dylib.lookupFunction<IntrovertStorageGetProfileC, IntrovertStorageGetProfileDart>('introvert_storage_get_profile'), () => FfiResult.dummy);
      _setProfile = safeLookup('set_profile', () => _dylib.lookupFunction<IntrovertStorageSetProfileC, IntrovertStorageSetProfileDart>('introvert_storage_set_profile'), (n, h, a, p) => FfiResult.dummy);
      _sendFile = safeLookup('send_file', () => _dylib.lookupFunction<IntrovertNetworkSendFileC, IntrovertNetworkSendFileDart>('introvert_network_send_file'), (p, f, g) => FfiResult.dummy);
      _cancelFileTransfer = safeLookup('cancel_file', () => _dylib.lookupFunction<IntrovertNetworkCancelFileTransferC, IntrovertNetworkCancelFileTransferDart>('introvert_network_cancel_file_transfer'), (id) => FfiResult.dummy);
      _forceNetworkRefresh = safeLookup('force_refresh', () => _dylib.lookupFunction<IntrovertNetworkForceRefreshC, IntrovertNetworkForceRefreshDart>('introvert_network_force_refresh'), () => FfiResult.dummy);
      _sendManualTelemetry = safeLookup('send_manual_telemetry', () => _dylib.lookupFunction<IntrovertSendManualTelemetryC, IntrovertSendManualTelemetryDart>('introvert_send_manual_telemetry'), () => FfiResult.dummy);
      _setConnectivityType = safeLookup('set_connectivity_type', () => _dylib.lookupFunction<IntrovertNetworkSetConnectivityTypeC, IntrovertNetworkSetConnectivityTypeDart>('introvert_network_set_connectivity_type'), (type) => FfiResult.dummy);
      _groupCreate = safeLookup('group_create', () => _dylib.lookupFunction<IntrovertGroupCreateC, IntrovertGroupCreateDart>('introvert_group_create'), (n, d, m) => FfiResult.dummy);
      _groupSendMessage = safeLookup('group_send', () => _dylib.lookupFunction<IntrovertGroupSendMessageC, IntrovertGroupSendMessageDart>('introvert_group_send_message'), (g, m, r) => FfiResult.dummy);
      _groupGetAll = safeLookup('group_get_all', () => _dylib.lookupFunction<IntrovertGroupGetAllC, IntrovertGroupGetAllDart>('introvert_group_get_all'), () => FfiResult.dummy);
      _groupGetMessages = safeLookup('group_get_msgs', () => _dylib.lookupFunction<IntrovertGroupGetMessagesC, IntrovertGroupGetMessagesDart>('introvert_group_get_messages'), (g) => FfiResult.dummy);
      _groupAddMember = safeLookup('group_add_member', () => _dylib.lookupFunction<IntrovertGroupAddMemberC, IntrovertGroupAddMemberDart>('introvert_group_add_member'), (g, p) => FfiResult.dummy);
      _groupApproveJoin = safeLookup('group_approve_join', () => _dylib.lookupFunction<IntrovertGroupApproveJoinC, IntrovertGroupApproveJoinDart>('introvert_group_approve_join'), (g, p, al, av, h) => FfiResult.dummy);
      _groupRejectJoin = safeLookup('group_reject_join', () => _dylib.lookupFunction<IntrovertGroupRejectJoinC, IntrovertGroupRejectJoinDart>('introvert_group_reject_join'), (g, p, r) => FfiResult.dummy);
      _groupRemoveMember = safeLookup('group_remove_member', () => _dylib.lookupFunction<IntrovertGroupRemoveMemberC, IntrovertGroupRemoveMemberDart>('introvert_group_remove_member'), (g, p) => FfiResult.dummy);
      _groupUpdateRole = safeLookup('group_update_role', () => _dylib.lookupFunction<IntrovertGroupUpdateRoleC, IntrovertGroupUpdateRoleDart>('introvert_group_update_role'), (g, p, r) => FfiResult.dummy);
      _groupPublishManifest = safeLookup('group_publish', () => _dylib.lookupFunction<IntrovertGroupPublishManifestC, IntrovertGroupPublishManifestDart>('introvert_group_publish_manifest'), (g, c) => FfiResult.dummy);
      _groupJoinByCode = safeLookup('group_join_code', () => _dylib.lookupFunction<IntrovertGroupJoinByCodeC, IntrovertGroupJoinByCodeDart>('introvert_group_join_by_code'), (c) => FfiResult.dummy);
      _groupDelete = safeLookup('group_delete', () => _dylib.lookupFunction<IntrovertGroupDeleteC, IntrovertGroupDeleteDart>('introvert_group_delete'), (g) => FfiResult.dummy);
      _groupGetPendingInvites = safeLookup('group_get_pending', () => _dylib.lookupFunction<IntrovertGroupGetPendingInvitesC, IntrovertGroupGetPendingInvitesDart>('introvert_group_get_pending_invites'), () => FfiResult.dummy);
      _groupAcceptInvite = safeLookup('group_accept_invite', () => _dylib.lookupFunction<IntrovertGroupAcceptInviteC, IntrovertGroupAcceptInviteDart>('introvert_group_accept_invite'), (g) => FfiResult.dummy);
      _groupDeclineInvite = safeLookup('group_decline_invite', () => _dylib.lookupFunction<IntrovertGroupDeclineInviteC, IntrovertGroupDeclineInviteDart>('introvert_group_decline_invite'), (g) => FfiResult.dummy);
      _groupMuteMember = safeLookup('group_mute', () => _dylib.lookupFunction<IntrovertGroupMuteMemberC, IntrovertGroupMuteMemberDart>('introvert_group_mute_member'), (g, p) => FfiResult.dummy);
      _groupUnmuteMember = safeLookup('group_unmute', () => _dylib.lookupFunction<IntrovertGroupUnmuteMemberC, IntrovertGroupUnmuteMemberDart>('introvert_group_unmute_member'), (g, p) => FfiResult.dummy);
      _groupGetMutedMembers = safeLookup('group_get_muted', () => _dylib.lookupFunction<IntrovertGroupGetMutedMembersC, IntrovertGroupGetMutedMembersDart>('introvert_group_get_muted_members'), (g) => FfiResult.dummy);
      _updateMessageStatus = safeLookup('update_msg_status', () => _dylib.lookupFunction<IntrovertStorageUpdateMessageStatusC, IntrovertStorageUpdateMessageStatusDart>('introvert_storage_update_message_status'), (m, s) => FfiResult.dummy);
      _updateMessageStatusForPeer = safeLookup('update_msg_status_peer', () => _dylib.lookupFunction<IntrovertStorageUpdateMessageStatusForPeerC, IntrovertStorageUpdateMessageStatusForPeerDart>('introvert_storage_update_message_status_for_peer'), (m, s) => FfiResult.dummy);
      _updateGroupMessageStatus = safeLookup('update_group_msg_status', () => _dylib.lookupFunction<IntrovertStorageUpdateGroupMessageStatusC, IntrovertStorageUpdateGroupMessageStatusDart>('introvert_storage_update_group_message_status'), (m, s) => FfiResult.dummy);
      _updateGroupMessageStatusById = safeLookup('update_group_msg_status_id', () => _dylib.lookupFunction<IntrovertStorageUpdateGroupMessageStatusByIdC, IntrovertStorageUpdateGroupMessageStatusByIdDart>('introvert_storage_update_group_message_status_by_id'), (m, s) => FfiResult.dummy);
      _getUnreadCounts = safeLookup('get_unread_counts', () => _dylib.lookupFunction<IntrovertStorageGetUnreadCountsC, IntrovertStorageGetUnreadCountsDart>('introvert_storage_get_unread_counts'), () => FfiResult.dummy);
      _sendAcknowledgement = safeLookup('send_ack', () => _dylib.lookupFunction<IntrovertNetworkSendAcknowledgementC, IntrovertNetworkSendAcknowledgementDart>('introvert_network_send_acknowledgement'), (p, m, s) => FfiResult.dummy);
      _updateContactAlias = safeLookup('update_contact_alias', () => _dylib.lookupFunction<IntrovertStorageUpdateContactAliasC, IntrovertStorageUpdateContactAliasDart>('introvert_storage_update_contact_alias'), (p, a) => FfiResult.dummy);
      _computeFileHash = safeLookup('compute_file_hash', () => _dylib.lookupFunction<IntrovertNetworkComputeFileHashC, IntrovertNetworkComputeFileHashDart>('introvert_network_compute_file_hash'), (f) => FfiResult.dummy);
      _nukeIdentity = safeLookup('nuke_identity', () => _dylib.lookupFunction<IntrovertNukeIdentityC, IntrovertNukeIdentityDart>('introvert_nuke_identity'), (db) => FfiResult.dummy);
      _driveAddFile = safeLookup('drive_add', () => _dylib.lookupFunction<IntrovertDriveAddFileC, IntrovertDriveAddFileDart>('introvert_drive_add_file'), (n, h, m, s, p) => FfiResult.dummy);
      _driveGetAll = safeLookup('drive_get_all', () => _dylib.lookupFunction<IntrovertDriveGetAllC, IntrovertDriveGetAllDart>('introvert_drive_get_all'), () => FfiResult.dummy);
      _driveGetByHash = safeLookup('drive_get_by_hash', () => _dylib.lookupFunction<IntrovertDriveGetByHashC, IntrovertDriveGetByHashDart>('introvert_drive_get_by_hash'), (h) => FfiResult.dummy);
      _driveDelete = safeLookup('drive_delete', () => _dylib.lookupFunction<IntrovertDriveDeleteC, IntrovertDriveDeleteDart>('introvert_drive_delete'), (h) => FfiResult.dummy);
      _driveAddFileWithFolder = safeLookup('drive_add_with_folder', () => _dylib.lookupFunction<IntrovertDriveAddFileWithFolderC, IntrovertDriveAddFileWithFolderDart>('introvert_drive_add_file_with_folder'), (n, h, m, s, p, f) => FfiResult.dummy);
      _driveUpdateFolder = safeLookup('drive_update_folder', () => _dylib.lookupFunction<IntrovertDriveUpdateFolderC, IntrovertDriveUpdateFolderDart>('introvert_drive_update_folder'), (h, f) => FfiResult.dummy);
      _getMeshCapacity = safeLookup('mesh_capacity', () => _dylib.lookupFunction<IntrovertGetMeshCapacityC, IntrovertGetMeshCapacityDart>('introvert_get_mesh_capacity'), () => 0);
      _getDiskSpace = safeLookup('get_disk_space', () => _dylib.lookupFunction<IntrovertGetDiskSpaceC, IntrovertGetDiskSpaceDart>('introvert_get_disk_space'), (path, total, free) => -1);
      _registerSeeder = safeLookup('register_seeder', () => _dylib.lookupFunction<IntrovertNetworkRegisterSeederC, IntrovertNetworkRegisterSeederDart>('introvert_network_register_seeder'), (t, p, h, s, g) => FfiResult.dummy);
      _startPull = safeLookup('start_pull', () => _dylib.lookupFunction<IntrovertNetworkStartPullC, IntrovertNetworkStartPullDart>('introvert_network_start_pull'), (p, t, n, m, h, s, r, g) => FfiResult.dummy);
      _notesCreate = safeLookup('notes_create', () => _dylib.lookupFunction<IntrovertNotesCreateC, IntrovertNotesCreateDart>('introvert_notes_create'), (id, t, c, tg, ip) => FfiResult.dummy);
      _notesUpdate = safeLookup('notes_update', () => _dylib.lookupFunction<IntrovertNotesUpdateC, IntrovertNotesUpdateDart>('introvert_notes_update'), (id, t, c, tg, ip) => FfiResult.dummy);
      _notesDelete = safeLookup('notes_delete', () => _dylib.lookupFunction<IntrovertNotesDeleteC, IntrovertNotesDeleteDart>('introvert_notes_delete'), (id) => FfiResult.dummy);
      _notesGet = safeLookup('notes_get', () => _dylib.lookupFunction<IntrovertNotesGetC, IntrovertNotesGetDart>('introvert_notes_get'), (id) => FfiResult.dummy);
      _notesGetAll = safeLookup('notes_get_all', () => _dylib.lookupFunction<IntrovertNotesGetAllC, IntrovertNotesGetAllDart>('introvert_notes_get_all'), () => FfiResult.dummy);
      _notesSearch = safeLookup('notes_search', () => _dylib.lookupFunction<IntrovertNotesSearchC, IntrovertNotesSearchDart>('introvert_notes_search'), (q) => FfiResult.dummy);
      _notesSaveVersion = safeLookup('notes_save_version', () => _dylib.lookupFunction<IntrovertNotesSaveVersionC, IntrovertNotesSaveVersionDart>('introvert_notes_save_version'), (nid, t, c, tg) => FfiResult.dummy);
      _notesGetVersions = safeLookup('notes_get_versions', () => _dylib.lookupFunction<IntrovertNotesGetVersionsC, IntrovertNotesGetVersionsDart>('introvert_notes_get_versions'), (nid) => FfiResult.dummy);
      _callHistoryLog = safeLookup('call_history_log', () => _dylib.lookupFunction<IntrovertCallHistoryLogC, IntrovertCallHistoryLogDart>('introvert_call_history_log'), (p, ct, mt, d, i) => FfiResult.dummy);
      _callHistoryGet = safeLookup('call_history_get', () => _dylib.lookupFunction<IntrovertCallHistoryGetC, IntrovertCallHistoryGetDart>('introvert_call_history_get'), (l) => FfiResult.dummy);
      _callHistoryCount = safeLookup('call_history_count', () => _dylib.lookupFunction<IntrovertCallHistoryCountC, IntrovertCallHistoryCountDart>('introvert_call_history_count'), () => FfiResult.dummy);
      _searchMessages = safeLookup('search_messages', () => _dylib.lookupFunction<IntrovertSearchMessagesC, IntrovertSearchMessagesDart>('introvert_search_messages'), (p, q) => FfiResult.dummy);
      _searchGroupMessages = safeLookup('search_group_messages', () => _dylib.lookupFunction<IntrovertSearchGroupMessagesC, IntrovertSearchGroupMessagesDart>('introvert_search_group_messages'), (g, q) => FfiResult.dummy);
      _sendTypingStart = safeLookup('send_typing_start', () => _dylib.lookupFunction<IntrovertSendTypingStartC, IntrovertSendTypingStartDart>('introvert_send_typing_start'), (p) => FfiResult.dummy);
      _sendTypingStop = safeLookup('send_typing_stop', () => _dylib.lookupFunction<IntrovertSendTypingStopC, IntrovertSendTypingStopDart>('introvert_send_typing_stop'), (p) => FfiResult.dummy);
      _getLastSeen = safeLookup('get_last_seen', () => _dylib.lookupFunction<IntrovertGetLastSeenC, IntrovertGetLastSeenDart>('introvert_get_last_seen'), (p) => FfiResult.dummy);
      _getAiMode = safeLookup('get_ai_mode', () => _dylib.lookupFunction<IntroClawGetAiModeC, IntroClawGetAiModeDart>('intro_claw_get_ai_mode'), () => 0);
      _setAiMode = safeLookup('set_ai_mode', () => _dylib.lookupFunction<IntroClawSetAiModeC, IntroClawSetAiModeDart>('intro_claw_set_ai_mode'), (m, k) => FfiResult.dummy);
      _getApiKey = safeLookup('get_api_key', () => _dylib.lookupFunction<IntroClawGetApiKeyC, IntroClawGetApiKeyDart>('intro_claw_get_api_key'), () => nullptr);
      _clawTriggerTick = safeLookup('claw_trigger_tick', () => _dylib.lookupFunction<IntroClawTriggerTickC, IntroClawTriggerTickDart>('intro_claw_trigger_tick'), (b) => FfiResult.dummy);
      _clawSetActive = safeLookup('claw_set_active', () => _dylib.lookupFunction<IntroClawSetActiveC, IntroClawSetActiveDart>('intro_claw_set_active'), (a) => FfiResult.dummy);
      _clawSetNodeMode = safeLookup('claw_set_node_mode', () => _dylib.lookupFunction<IntroClawSetNodeModeC, IntroClawSetNodeModeDart>('intro_claw_set_node_mode'), (e) => FfiResult.dummy);
      _clawGetStatus = safeLookup('claw_get_status', () => _dylib.lookupFunction<IntroClawGetStatusC, IntroClawGetStatusDart>('intro_claw_get_status'), () => FfiResult.dummy);
      _clawGetEndpoint = safeLookup('claw_get_endpoint', () => _dylib.lookupFunction<IntroClawGetEndpointC, IntroClawGetEndpointDart>('intro_claw_get_endpoint'), () => nullptr);
      _clawSetEndpoint = safeLookup('claw_set_endpoint', () => _dylib.lookupFunction<IntroClawSetEndpointC, IntroClawSetEndpointDart>('intro_claw_set_endpoint'), (e) => FfiResult.dummy);
      _clawProcessQuery = safeLookup('claw_process_query', () => _dylib.lookupFunction<IntroClawProcessQueryC, IntroClawProcessQueryDart>('intro_claw_process_query'), (q) => FfiResult.dummy);
      _clawRunNetworkRecon = safeLookup('claw_run_network_recon', () => _dylib.lookupFunction<IntroClawRunNetworkReconC, IntroClawRunNetworkReconDart>('intro_claw_run_network_recon'), () => FfiResult.dummy);
      _clawHealPeer = safeLookup('claw_heal_peer', () => _dylib.lookupFunction<IntroClawHealPeerC, IntroClawHealPeerDart>('intro_claw_heal_peer'), (p) => FfiResult.dummy);
      _clawGetActivityLog = safeLookup('claw_get_activity_log', () => _dylib.lookupFunction<IntroClawGetActivityLogC, IntroClawGetActivityLogDart>('intro_claw_get_activity_log'), () => FfiResult.dummy);
      _clawVoipStartCall = safeLookup('claw_voip_start_call', () => _dylib.lookupFunction<IntroClawVoipStartCallC, IntroClawVoipStartCallDart>('intro_claw_voip_start_call'), (p, v) => FfiResult.dummy);
      _clawVoipEndCall = safeLookup('claw_voip_end_call', () => _dylib.lookupFunction<IntroClawVoipEndCallC, IntroClawVoipEndCallDart>('intro_claw_voip_end_call'), () => FfiResult.dummy);
      _clawVoipRecordSample = safeLookup('claw_voip_record_sample', () => _dylib.lookupFunction<IntroClawVoipRecordSampleC, IntroClawVoipRecordSampleDart>('intro_claw_voip_record_sample'), (r, p, j, b, rel, c) => FfiResult.dummy);
      _clawVoipGetQuality = safeLookup('claw_voip_get_quality', () => _dylib.lookupFunction<IntroClawVoipGetQualityC, IntroClawVoipGetQualityDart>('intro_claw_voip_get_quality'), () => FfiResult.dummy);
      _clawVoipGetDowngradeRecommendation = safeLookup('claw_voip_get_downgrade_recommendation', () => _dylib.lookupFunction<IntroClawVoipGetDowngradeRecommendationC, IntroClawVoipGetDowngradeRecommendationDart>('intro_claw_voip_get_downgrade_recommendation'), () => FfiResult.dummy);
      _clawSetActiveChat = safeLookup('claw_set_active_chat', () => _dylib.lookupFunction<IntroClawSetActiveChatC, IntroClawSetActiveChatDart>('intro_claw_set_active_chat'), (c, p, g) => FfiResult.dummy);
      _clawClearActiveChat = safeLookup('claw_clear_active_chat', () => _dylib.lookupFunction<IntroClawClearActiveChatC, IntroClawClearActiveChatDart>('intro_claw_clear_active_chat'), () => FfiResult.dummy);
      _clawSetActiveGroupMembers = safeLookup('claw_set_active_group_members', () => _dylib.lookupFunction<IntroClawSetActiveGroupMembersC, IntroClawSetActiveGroupMembersDart>('intro_claw_set_active_group_members'), (m) => FfiResult.dummy);
      _clawOnAppLaunch = safeLookup('claw_on_app_launch', () => _dylib.lookupFunction<IntroClawOnAppLaunchC, IntroClawOnAppLaunchDart>('intro_claw_on_app_launch'), () => FfiResult.dummy);

      // Elevated Messages
      _elevateMessage = safeLookup('elevate_message', () => _dylib.lookupFunction<IntrovertElevateMessageC, IntrovertElevateMessageDart>('introvert_elevate_message'), (c, m, co, s, i) => FfiResult.dummy);
      _unelevateMessage = safeLookup('unelevate_message', () => _dylib.lookupFunction<IntrovertUnelevateMessageC, IntrovertUnelevateMessageDart>('introvert_unelevate_message'), (c, m) => FfiResult.dummy);
      _getElevatedMessages = safeLookup('get_elevated_messages', () => _dylib.lookupFunction<IntrovertGetElevatedMessagesC, IntrovertGetElevatedMessagesDart>('introvert_get_elevated_messages'), (c) => FfiResult.dummy);
      _isMessageElevated = safeLookup('is_message_elevated', () => _dylib.lookupFunction<IntrovertIsMessageElevatedC, IntrovertIsMessageElevatedDart>('introvert_is_message_elevated'), (c, m) => FfiResult.dummy);

      // Optimized last message queries
      _getLastMessage = safeLookup('get_last_message', () => _dylib.lookupFunction<IntrovertStorageGetLastMessageC, IntrovertStorageGetLastMessageDart>('introvert_storage_get_last_message'), (p) => FfiResult.dummy);
      _getLastGroupMessage = safeLookup('get_last_group_message', () => _dylib.lookupFunction<IntrovertStorageGetLastGroupMessageC, IntrovertStorageGetLastGroupMessageDart>('introvert_storage_get_last_group_message'), (g) => FfiResult.dummy);
      _getLastMessagesAll = safeLookup('get_last_messages_all', () => _dylib.lookupFunction<IntrovertStorageGetLastMessagesAllC, IntrovertStorageGetLastMessagesAllDart>('introvert_storage_get_last_messages_all'), () => FfiResult.dummy);
      _getLastGroupMessagesAll = safeLookup('get_last_group_messages_all', () => _dylib.lookupFunction<IntrovertStorageGetLastGroupMessagesAllC, IntrovertStorageGetLastGroupMessagesAllDart>('introvert_storage_get_last_group_messages_all'), () => FfiResult.dummy);

      // Daily Rewards
      _dailyRewardGetStatus = safeLookup('daily_reward_get_status', () => _dylib.lookupFunction<IntrovertDailyRewardGetStatusC, IntrovertDailyRewardGetStatusDart>('introvert_daily_reward_get_status'), () => FfiResult.dummy);
      _dailyRewardGetHistory = safeLookup('daily_reward_get_history', () => _dylib.lookupFunction<IntrovertDailyRewardGetHistoryC, IntrovertDailyRewardGetHistoryDart>('introvert_daily_reward_get_history'), (d) => FfiResult.dummy);
      _dailyRewardRecordActivity = safeLookup('daily_reward_record_activity', () => _dylib.lookupFunction<IntrovertDailyRewardRecordActivityC, IntrovertDailyRewardRecordActivityDart>('introvert_daily_reward_record_activity'), (p, l) => FfiResult.dummy);
      _dailyRewardUpdateWeights = safeLookup('daily_reward_update_weights', () => _dylib.lookupFunction<IntrovertDailyRewardUpdateWeightsC, IntrovertDailyRewardUpdateWeightsDart>('introvert_daily_reward_update_weights'), (p, l) => FfiResult.dummy);
      _dailyRewardUpdateAntiGaming = safeLookup('daily_reward_update_anti_gaming', () => _dylib.lookupFunction<IntrovertDailyRewardUpdateAntiGamingC, IntrovertDailyRewardUpdateAntiGamingDart>('introvert_daily_reward_update_anti_gaming'), (p, l) => FfiResult.dummy);
      _dailyRewardGetRealtimeEarnings = safeLookup('daily_reward_get_realtime_earnings', () => _dylib.lookupFunction<IntrovertDailyRewardGetRealtimeEarningsC, IntrovertDailyRewardGetRealtimeEarningsDart>('introvert_daily_reward_get_realtime_earnings'), () => FfiResult.dummy);
      try {
        _getRewardsState = loadGetRewardsState(_dylib);
      } catch (_) {
        debugPrint('⚠️ get_current_rewards_state not available in native library');
      }

      debugPrint('✅ All native functions bound successfully.');
    } catch (e) {
      debugPrint('❌ Error binding native functions: $e');
    }
  }

  void forceNetworkRefresh() {
    _forceNetworkRefresh();
  }

  /// Manually trigger telemetry send to RBN for the current epoch cycle.
  /// Returns true if the command was dispatched successfully.
  bool sendManualTelemetry() {
    final result = _sendManualTelemetry();
    return result.code == 0;
  }

  // Inform native layer of current connectivity type (0=unknown,1=wifi,2=mobile,3=ethernet,4=bluetooth)
  void setConnectivityType(ConnectivityResult connectivity) {
    int type;
    switch (connectivity) {
      case ConnectivityResult.wifi:
        type = 1;
        break;
      case ConnectivityResult.mobile:
        type = 2;
        break;
      case ConnectivityResult.ethernet:
        type = 3;
        break;
      case ConnectivityResult.bluetooth:
        type = 4;
        break;
      case ConnectivityResult.vpn:
        type = 5;
        break;
      default:
        type = 0;
    }
    _setConnectivityType(type);
  }

  void createGroup(String name, String description, List<String> members) {
    using((Arena arena) {
      _groupCreate(
        name.toNativeUtf8(allocator: arena),
        description.toNativeUtf8(allocator: arena),
        json.encode(members).toNativeUtf8(allocator: arena),
      );
    });
  }

  void deleteGroup(String groupId) {
    using((Arena arena) {
      _groupDelete(
        groupId.toNativeUtf8(allocator: arena),
      );
    });
  }

  List<dynamic> getPendingGroupInvites() {
    final res = _groupGetPendingInvites();
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void acceptGroupInvite(String groupId) {
    using((Arena arena) {
      _groupAcceptInvite(
        groupId.toNativeUtf8(allocator: arena),
      );
    });
  }

  void declineGroupInvite(String groupId) {
    using((Arena arena) {
      _groupDeclineInvite(
        groupId.toNativeUtf8(allocator: arena),
      );
    });
  }

  void registerSeeder(String transferId, String filePath, String fileHash, int totalSize, [String? groupId]) {
    using((Arena arena) {
      _registerSeeder(
        transferId.toNativeUtf8(allocator: arena),
        filePath.toNativeUtf8(allocator: arena),
        fileHash.toNativeUtf8(allocator: arena),
        totalSize,
        (groupId ?? "").toNativeUtf8(allocator: arena),
      );
    });
  }

  void startPull(String peerId, String transferId, String filename, String mimeType, String fileHash, int totalSize, bool isRelayed, [String? groupId]) {
    using((Arena arena) {
      _startPull(
        peerId.toNativeUtf8(allocator: arena),
        transferId.toNativeUtf8(allocator: arena),
        filename.toNativeUtf8(allocator: arena),
        mimeType.toNativeUtf8(allocator: arena),
        fileHash.toNativeUtf8(allocator: arena),
        totalSize,
        isRelayed,
        groupId != null ? groupId.toNativeUtf8(allocator: arena) : nullptr,
      );
    });
  }

  void sendGroupMessage(String groupId, String message, [String? replyTo]) {
    using((Arena arena) {
      _groupSendMessage(
        groupId.toNativeUtf8(allocator: arena),
        message.toNativeUtf8(allocator: arena),
        (replyTo ?? "").toNativeUtf8(allocator: arena),
      );
    });
  }

  List<dynamic> getAllGroups() {
    final res = _groupGetAll();
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  List<dynamic> getGroupMessages(String groupId) {
    late FfiResult res;
    using((Arena arena) => res = _groupGetMessages(groupId.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void addGroupMember(String groupId, String peerId) {
    using((Arena arena) {
      _groupAddMember(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
      );
    });
  }

  void approveGroupJoin(String groupId, String peerId, String? alias, String? avatar, String? handle) {
    using((Arena arena) {
      _groupApproveJoin(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
        alias != null ? alias.toNativeUtf8(allocator: arena) : nullptr,
        avatar != null ? avatar.toNativeUtf8(allocator: arena) : nullptr,
        handle != null ? handle.toNativeUtf8(allocator: arena) : nullptr,
      );
    });
  }

  void rejectGroupJoin(String groupId, String peerId, String reason) {
    using((Arena arena) {
      _groupRejectJoin(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
        reason.toNativeUtf8(allocator: arena),
      );
    });
  }

  void removeGroupMember(String groupId, String peerId) {
    using((Arena arena) {
      _groupRemoveMember(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
      );
    });
  }

  void updateGroupRole(String groupId, String peerId, int role) {
    using((Arena arena) {
      _groupUpdateRole(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
        role,
      );
    });
  }

  void muteMember(String groupId, String peerId) {
    using((Arena arena) {
      _groupMuteMember(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
      );
    });
  }

  void unmuteMember(String groupId, String peerId) {
    using((Arena arena) {
      _groupUnmuteMember(
        groupId.toNativeUtf8(allocator: arena),
        peerId.toNativeUtf8(allocator: arena),
      );
    });
  }

  List<String> getGroupMutedMembers(String groupId) {
    late FfiResult res;
    using((Arena arena) => res = _groupGetMutedMembers(groupId.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return [];
      final jsonStr = utf8.decode(res.data.asTypedList(res.len));
      return List<String>.from(json.decode(jsonStr));
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void publishGroupManifest(String groupId, String code) {
    using((Arena arena) {
      _groupPublishManifest(
        groupId.toNativeUtf8(allocator: arena),
        code.toNativeUtf8(allocator: arena),
      );
    });
  }

  void joinMeshByCode(String code) {
    using((Arena arena) {
      _groupJoinByCode(
        code.toNativeUtf8(allocator: arena),
      );
    });
  }

  void updateMessageStatus(String msgId, int status) {
    using((Arena arena) => _updateMessageStatus(msgId.toNativeUtf8(allocator: arena), status));
  }

  void updateMessageStatusForPeer(String peerId, int status) {
    using((Arena arena) => _updateMessageStatusForPeer(peerId.toNativeUtf8(allocator: arena), status));
  }

  void updateGroupMessageStatus(String groupId, int status) {
    using((Arena arena) => _updateGroupMessageStatus(groupId.toNativeUtf8(allocator: arena), status));
  }

  void updateGroupMessageStatusById(String msgId, int status) {
    using((Arena arena) => _updateGroupMessageStatusById(msgId.toNativeUtf8(allocator: arena), status));
  }

  Map<String, int> getUnreadCounts() {
    final res = _getUnreadCounts();
    try {
      if (res.code != 0) return {};
      final jsonStr = utf8.decode(res.data.asTypedList(res.len));
      final Map<String, dynamic> raw = json.decode(jsonStr);
      return raw.map((k, v) => MapEntry(k, v as int));
    } catch (e) {
      debugPrint("❌ Error decoding unread counts: $e");
      return {};
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void sendAcknowledgement(String peerId, String msgId, int status) {
    using((Arena arena) => _sendAcknowledgement(
      peerId.toNativeUtf8(allocator: arena),
      msgId.toNativeUtf8(allocator: arena),
      status
    ));
  }

  void updateContactAlias(String peerId, String alias) {
    using((Arena arena) => _handleFfiResult(_updateContactAlias(
      peerId.toNativeUtf8(allocator: arena),
      alias.toNativeUtf8(allocator: arena),
    ), context: "Update Contact Alias"));
  }

  void nukeIdentity(String dbPath) {
    closeCallables();
    _disposeStreams();
    using((Arena arena) => _handleFfiResult(_nukeIdentity(dbPath.toNativeUtf8(allocator: arena)), context: "Nuke Identity"));
  }

  void _disposeStreams() {
    _networkStreamController.close();
    _mediaStreamController.close();
    _transferStreamController.close();
    _economyStreamController.close();
    _swarmStatsStreamController.close();
    _telemetryAckStreamController.close();
  }

  void driveAddFile(String name, String hash, String mime, int size, String path) {
    using((Arena arena) => _handleFfiResult(_driveAddFile(
      name.toNativeUtf8(allocator: arena),
      hash.toNativeUtf8(allocator: arena),
      mime.toNativeUtf8(allocator: arena),
      size,
      path.toNativeUtf8(allocator: arena),
    ), context: "Drive Add File"));
  }

  void driveAddFileWithFolder(String name, String hash, String mime, int size, String path, String folder) {
    using((Arena arena) => _handleFfiResult(_driveAddFileWithFolder(
      name.toNativeUtf8(allocator: arena),
      hash.toNativeUtf8(allocator: arena),
      mime.toNativeUtf8(allocator: arena),
      size,
      path.toNativeUtf8(allocator: arena),
      folder.toNativeUtf8(allocator: arena),
    ), context: "Drive Add File With Folder"));
  }

  void driveUpdateFolder(String hash, String folder) {
    using((Arena arena) => _handleFfiResult(_driveUpdateFolder(
      hash.toNativeUtf8(allocator: arena),
      folder.toNativeUtf8(allocator: arena),
    ), context: "Drive Update Folder"));
  }

  List<dynamic> driveGetAll() {
    final res = _driveGetAll();
    if (res.code != 0) return [];
    try {
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  Map<String, dynamic> driveGetByHash(String fileHash) {
    final res = using((Arena arena) => _driveGetByHash(fileHash.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return {};
      final decoded = json.decode(utf8.decode(res.data.cast<Uint8>().asTypedList(res.len)));
      if (decoded is Map<String, dynamic>) return decoded;
      return {};
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void driveDelete(String hash) {
    using((Arena arena) => _handleFfiResult(_driveDelete(hash.toNativeUtf8(allocator: arena)), context: "Drive Delete"));
  }

  Map<String, int> getDiskSpace(String path) {
    return using((Arena arena) {
      final pathPtr = path.toNativeUtf8(allocator: arena);
      final totalBytesPtr = arena<Uint64>();
      final freeBytesPtr = arena<Uint64>();
      
      final res = _getDiskSpace(pathPtr, totalBytesPtr, freeBytesPtr);
      if (res == 0) {
        return {
          'total': totalBytesPtr.value,
          'free': freeBytesPtr.value,
        };
      } else {
        return {
          'total': 0,
          'free': 0,
        };
      }
    });
  }

  int getMeshCapacity() => _getMeshCapacity();

  void startNetwork({int port = 0, bool relay = false, int maxConn = 1024, int liveness = 600}) {
    _initializeUnifiedCallable();
    debugPrint('📡 Initializing Unified Network Plane (Port: $port, Relay: $relay)...');
    _handleFfiResult(_networkStart(_unifiedCallable!.nativeFunction, port, relay, maxConn, liveness), context: "Network Start");
  }

  void startEconomyMonitoring(void Function(Map<String, dynamic> stats) onUpdate) {
    _initializeUnifiedCallable();
    debugPrint('💰 Subscribing to Economy Monitoring...');
    // We already have a unified listener, just trigger the monitoring in Rust
    _handleFfiResult(_economyStartMonitoring(_unifiedCallable!.nativeFunction), context: "Economy Monitoring");
    
    // Also listen to the internal stream for updates
    _economyStreamController.stream.listen(onUpdate);
  }


  void startWormholeInvite() => _handleFfiResult(_wormholeStart(), context: "Wormhole Start");

  void joinWormholeInvite(String code) {
    using((Arena arena) => _handleFfiResult(_wormholeJoin(code.toNativeUtf8(allocator: arena)), context: "Wormhole Join"));
  }

  void abortWormhole() => _handleFfiResult(_wormholeAbort(), context: "Wormhole Abort");

  String generateMnemonic() {
    final ptr = _generateMnemonic();
    if (ptr.address == 0) {
      if (Platform.isIOS) {
        throw UnsupportedError(
          'Native library not linked for this iOS target.\n\n'
          'The Rust library (libintrovert) must be compiled for '
          'aarch64-apple-ios-sim to run on the iOS Simulator, or '
          'aarch64-apple-ios for a real device.\n\n'
          'Run: cargo build --release --target aarch64-apple-ios-sim\n'
          'Then copy the output .a to ios/libs/libintrovert_simulator.a',
        );
      }
      throw Exception('Mnemonic generation failed');
    }
    try { return ptr.toDartString(); } finally { _freeString(ptr); }
  }

  Uint8List mnemonicToSeed(String phrase) {
    return using((Arena arena) {
      final res = _mnemonicToSeed(phrase.toNativeUtf8(allocator: arena));
      try {
        if (res.code != 0) {
          String errMsg = res.data != nullptr && res.len > 0 ? utf8.decode(res.data.asTypedList(res.len)) : "Unknown error";
          throw Exception("Seed derivation failed (${res.code}): $errMsg");
        }
        return Uint8List.fromList(res.data.asTypedList(res.len));
      } finally {
        if (res.data != nullptr) _freeBinary(res.data, res.len);
      }
    });
  }

  Map<String, String> deriveIdentifiers(Uint8List seed) {
    return using((Arena arena) {
      final seedPtr = arena<Uint8>(32);
      seedPtr.asTypedList(32).setAll(0, seed);
      final res = _deriveIdentifiers(seedPtr);
      try {
        if (res.code != 0) {
          String errMsg = res.data != nullptr && res.len > 0 ? utf8.decode(res.data.asTypedList(res.len)) : "Unknown error";
          throw Exception("Identifiers derivation failed (${res.code}): $errMsg");
        }
        final jsonStr = utf8.decode(res.data.asTypedList(res.len));
        final decoded = json.decode(jsonStr) as Map<String, dynamic>;
        return {
          'peer_id': decoded['peer_id']?.toString() ?? '',
          'solana_address': decoded['solana_address']?.toString() ?? '',
        };
      } finally {
        if (res.data != nullptr) _freeBinary(res.data, res.len);
      }
    });
  }

  void startEngine(Uint8List seed, String dbPath) {
    _initializeUnifiedCallable();
    using((Arena arena) {
      final seedPtr = arena<Uint8>(32);
      for (var i = 0; i < 32; i++) {
        seedPtr[i] = seed[i];
      }
      _handleFfiResult(_engineStart(seedPtr, dbPath.toNativeUtf8(allocator: arena)), context: "Engine Start");
    });
  }

  void stopEngine() {
    closeCallables();
    _handleFfiResult(_engineStop(), context: "Engine Stop");
  }

  void closeCallables() {
    _unifiedCallable?.close();
    _unifiedCallable = null;
    debugPrint("✅ IntrovertClient: Closed unified native callable.");
  }

  String? get localPeerId => getPeerId();

  String? getPeerId() {
    final ptr = _getPeerId();
    if (ptr.address == 0) return null;
    try { return ptr.toDartString(); } finally { _freeString(ptr); }
  }

  List<dynamic> getContacts() {
    final res = _getContacts();
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  Future<void> deleteContact(String id) async => using((Arena arena) => _handleFfiResult(_deleteContact(id.toNativeUtf8(allocator: arena)), context: "Delete Contact"));
  Future<void> deleteChat(String id) async => using((Arena arena) => _handleFfiResult(_deleteChat(id.toNativeUtf8(allocator: arena)), context: "Delete Chat"));
  Future<void> clearAllContacts() async => _handleFfiResult(_clearContacts(), context: "Clear Contacts");
  void setProfileTier(int tier) => _handleFfiResult(_setProfileTier(tier), context: "Set Profile Tier");

  Future<String> sendMessage(String id, String msg, [String? replyTo]) async {
    final comp = Completer<String>();
    final cb = NativeCallable<NativeFfiCallback>.listener((FfiResult res) {
      if (res.code == 0) {
        if (res.len > 0) {
          final data = res.data.cast<Uint8>().asTypedList(res.len);
          final msgId = utf8.decode(data);
          comp.complete(msgId);
        } else {
          comp.complete("");
        }
      } else {
        comp.completeError(Exception("Send failed"));
      }
      if (res.len > 0) _freeBinary(res.data, res.len);
    });
    using((Arena arena) => _networkSendMessage(
      id.toNativeUtf8(allocator: arena), 
      msg.toNativeUtf8(allocator: arena), 
      (replyTo ?? "").toNativeUtf8(allocator: arena),
      cb.nativeFunction
    ));
    final msgId = await comp.future;
    cb.close();
    return msgId;
  }

  void addAddress(String id, String address) => using((Arena arena) => _handleFfiResult(_addAddress(id.toNativeUtf8(allocator: arena), address.toNativeUtf8(allocator: arena)), context: "Add Address"));

  Future<void> storeMessage(String id, String msg, {bool isMe = false}) async {
    final comp = Completer<void>();
    final cb = NativeCallable<NativeFfiCallback>.listener((FfiResult res) {
      if (res.code == 0) {
        comp.complete();
      } else {
        comp.completeError(Exception("Store failed"));
      }
      if (res.len > 0) _freeBinary(res.data, res.len);
    });
    using((Arena arena) => _storeMessageAsync(id.toNativeUtf8(allocator: arena), msg.toNativeUtf8(allocator: arena), isMe, cb.nativeFunction));
    await comp.future; cb.close();
  }

  List<dynamic> getMessages(String peerId) {
    late FfiResult result;
    using((Arena arena) => result = _getMessages(peerId.toNativeUtf8(allocator: arena)));
    
    try {
      if (result.code == 0 && result.len > 0) {
        final data = result.data.cast<Uint8>().asTypedList(result.len);
        final jsonStr = utf8.decode(data);
        return json.decode(jsonStr);
      }
      return [];
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  /// Paginated version: returns the most recent `limit` messages starting from `offset`.
  /// offset=0, limit=50 returns the last 50 messages.
  List<dynamic> getMessagesPaginated(String peerId, {int offset = 0, int limit = 50}) {
    late FfiResult result;
    using((Arena arena) => result = _getMessagesPaginated(peerId.toNativeUtf8(allocator: arena), offset, limit));
    
    try {
      if (result.code == 0 && result.len > 0) {
        final data = result.data.cast<Uint8>().asTypedList(result.len);
        final jsonStr = utf8.decode(data);
        return json.decode(jsonStr);
      }
      return [];
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  void establishSecureSession(String id) => using((Arena arena) => _handleFfiResult(_establishSecureSession(id.toNativeUtf8(allocator: arena)), context: "Secure Session"));

  Future<void> initiateWebRtc(String peerId, int mediaType) async {
    final completer = Completer<void>();
    final callback = NativeCallable<NativeFfiCallback>.listener((FfiResult result) {
      if (result.code == 0) {
        completer.complete();
      } else {
        completer.completeError(Exception("WebRTC error (${result.code})"));
      }
      if (result.len > 0) _freeBinary(result.data, result.len);
    });
    using((Arena arena) => _networkInitiateWebRtc(peerId.toNativeUtf8(allocator: arena), mediaType, callback.nativeFunction));
    await completer.future; callback.close();
  }

  void closeWebRtc(String peerId) => using((Arena arena) => _handleFfiResult(_closeWebRtc(peerId.toNativeUtf8(allocator: arena)), context: "Close WebRTC"));
  void renegotiateWebRtc(String peerId) => using((Arena arena) => _handleFfiResult(_renegotiateWebRtc(peerId.toNativeUtf8(allocator: arena)), context: "Renegotiate WebRTC"));
  void acceptCall(String peerId, int mediaType) => using((Arena arena) => _handleFfiResult(_acceptCall(peerId.toNativeUtf8(allocator: arena), mediaType), context: "Accept Call"));
  void rejectCall(String peerId) => using((Arena arena) => _handleFfiResult(_rejectCall(peerId.toNativeUtf8(allocator: arena)), context: "Reject Call"));

  /// Forward a flutter_webrtc SDP/ICE signal JSON to a remote peer via the Rust mesh.
  void sendWebRtcSignal(String peerId, Uint8List jsonBytes) {
    final json = String.fromCharCodes(jsonBytes);
    using((Arena arena) {
      _sendNativeSignal(
        peerId.toNativeUtf8(allocator: arena),
        json.toNativeUtf8(allocator: arena),
      );
    });
  }

  void fetchMailbox() => _handleFfiResult(_fetchMailbox(), context: "Fetch Mailbox");
  void startMediaStream(String id, int type) => using((Arena arena) => _handleFfiResult(_startMediaStream(id.toNativeUtf8(allocator: arena), type), context: "Media Stream"));

  void setAnchorMode(bool enabled) {
    _handleFfiResult(_setAnchorMode(enabled), context: "Set Anchor Mode");
    // When anchor mode is enabled, also enable Intro-Claw node mode
    // for aggressive optimizations (proactive caching, bandwidth management)
    setIntroClawNodeMode(enabled);
  }
  bool isAnchorModeEnabled() => _getAnchorMode() == 1;

  // --- Intro-Claw AI Engine Mode ---
  int getIntroClawAiMode() => _getAiMode();
  
  void setIntroClawAiMode(int mode, {String apiKey = ''}) {
    using((Arena arena) {
      _handleFfiResult(
        _setAiMode(mode, apiKey.toNativeUtf8(allocator: arena)),
        context: "Set Intro-Claw AI Mode",
      );
    });
  }
  
  String getIntroClawApiKey() {
    final ptr = _getApiKey();
    if (ptr.address == 0) return '';
    try { return ptr.toDartString(); } finally { _freeString(ptr); }
  }

  // --- Intro-Claw Automation Methods ---
  void triggerIntroClawTick({bool isMobileData = false}) => _handleFfiResult(_clawTriggerTick(isMobileData), context: "IntroClaw Tick");
  void setIntroClawActive(bool active) => _handleFfiResult(_clawSetActive(active), context: "IntroClaw Active");
  void setIntroClawNodeMode(bool enabled) => _handleFfiResult(_clawSetNodeMode(enabled), context: "IntroClaw Node Mode");
  String getIntroClawStatus() {
    final result = _clawGetStatus();
    try {
      return String.fromCharCodes(result.data.cast<Uint8>().asTypedList(result.len));
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  String getIntroClawEndpoint() {
    final ptr = _clawGetEndpoint();
    if (ptr.address == 0) return '';
    try { return ptr.toDartString(); } finally { _freeString(ptr); }
  }

  void setIntroClawEndpoint(String endpoint) {
    using((Arena arena) {
      _handleFfiResult(_clawSetEndpoint(endpoint.toNativeUtf8(allocator: arena)), context: "Set IntroClaw Endpoint");
    });
  }

  String processAssistantQuery(String query) {
    final result = using((Arena arena) => _clawProcessQuery(query.toNativeUtf8(allocator: arena)));
    try {
      return String.fromCharCodes(result.data.cast<Uint8>().asTypedList(result.len));
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  String runNetworkRecon() {
    final result = _clawRunNetworkRecon();
    try {
      return String.fromCharCodes(result.data.cast<Uint8>().asTypedList(result.len));
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  String healPeer(String peerId) {
    final result = using((Arena arena) => _clawHealPeer(peerId.toNativeUtf8(allocator: arena)));
    try {
      return String.fromCharCodes(result.data.cast<Uint8>().asTypedList(result.len));
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  String getIntroClawActivityLog() {
    final result = _clawGetActivityLog();
    try {
      return String.fromCharCodes(result.data.cast<Uint8>().asTypedList(result.len));
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  void voipStartCall(String peerId, bool isVideo) {
    using((Arena arena) {
      _handleFfiResult(
        _clawVoipStartCall(peerId.toNativeUtf8(allocator: arena), isVideo ? 1 : 0),
        context: "VoIP Start Call",
      );
    });
  }

  void voipEndCall() {
    _handleFfiResult(_clawVoipEndCall(), context: "VoIP End Call");
  }

  void setActiveChat(String chatId, String? peerId, bool isGroup) {
    using((Arena arena) {
      _handleFfiResult(
        _clawSetActiveChat(
          chatId.toNativeUtf8(allocator: arena),
          (peerId ?? '').toNativeUtf8(allocator: arena),
          isGroup ? 1 : 0,
        ),
        context: "IntroClaw Set Active Chat",
      );
    });
  }

  void clearActiveChat() {
    _handleFfiResult(_clawClearActiveChat(), context: "IntroClaw Clear Active Chat");
  }

  void setActiveGroupMembers(List<String> members) {
    using((Arena arena) {
      final jsonStr = jsonEncode(members);
      _handleFfiResult(
        _clawSetActiveGroupMembers(jsonStr.toNativeUtf8(allocator: arena)),
        context: "IntroClaw Set Active Group Members",
      );
    });
  }

  void onAppLaunch() {
    _handleFfiResult(_clawOnAppLaunch(), context: "IntroClaw On App Launch");
  }

  void voipRecordSample(int rttMs, double packetLossPct, int jitterMs, int bitrateKbps, bool isRelayed, String codec) {
    using((Arena arena) {
      _handleFfiResult(
        _clawVoipRecordSample(rttMs, packetLossPct, jitterMs, bitrateKbps, isRelayed ? 1 : 0, codec.toNativeUtf8(allocator: arena)),
        context: "VoIP Record Sample",
      );
    });
  }

  String voipGetQuality() {
    final result = _clawVoipGetQuality();
    try {
      return String.fromCharCodes(result.data.cast<Uint8>().asTypedList(result.len));
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  /// Get VoIP downgrade recommendation
  /// Returns: "none", "audio_only", "low_bitrate"
  String voipGetDowngradeRecommendation() {
    final result = _clawVoipGetDowngradeRecommendation();
    try {
      return String.fromCharCodes(result.data.cast<Uint8>().asTypedList(result.len));
    } finally {
      if (result.len > 0) _freeBinary(result.data, result.len);
    }
  }

  void setTunnelMode(bool enabled) => _handleFfiResult(_setTunnelMode(enabled), context: "Set Tunnel Mode");
  bool isTunnelModeEnabled() => _getTunnelMode() == 1;
  void recheckConnection(String peerId) {
    using((Arena arena) {
      _handleFfiResult(
        _recheckConnection(peerId.toNativeUtf8(allocator: arena)),
        context: "Recheck Connection",
      );
    });
  }

  void requestSwarmStats() {
    _handleFfiResult(_requestSwarmStats(), context: "Request Swarm Stats");
  }

  void pollPeerProfile(String peerId) {
    using((Arena arena) {
      _handleFfiResult(_pollPeerProfile(peerId.toNativeUtf8(allocator: arena)), context: "Poll Peer Profile");
    });
  }

  void syncChatMessages(String peerId, String chatId, bool isGroup, {bool isFull = false}) {
    using((Arena arena) {
      _handleFfiResult(
        _syncChatMessages(
          peerId.toNativeUtf8(allocator: arena),
          chatId.toNativeUtf8(allocator: arena),
          isGroup ? 1 : 0,
          isFull ? 1 : 0,
        ),
        context: "Sync Chat Messages",
      );
    });
  }

  void resolveHandle(String handle) {
    var h = handle.trim();
    if (!h.startsWith("i@")) {
      h = "i@$h";
    }
    using((Arena arena) => _handleFfiResult(_resolveHandle(h.toNativeUtf8(allocator: arena)), context: "Resolve Handle"));
  }

  void sendDirectInvite(String peerId) {
    using((Arena arena) => _handleFfiResult(_sendDirectInvite(peerId.toNativeUtf8(allocator: arena)), context: "Send Direct Invite"));
  }

  void registerPushToken(String deviceType, String token) {
    using((Arena arena) => _handleFfiResult(_registerPushToken(deviceType.toNativeUtf8(allocator: arena), token.toNativeUtf8(allocator: arena)), context: "Register Push Token"));
  }

  void setRetention(String targetId, int seconds, bool isGroup) {
    using((Arena arena) => _handleFfiResult(_setRetention(
      targetId.toNativeUtf8(allocator: arena),
      seconds,
      isGroup,
    ), context: "Set Retention"));
  }

  void deleteMessage(String targetId, String msgId, bool isGroup, {bool deletedByAdmin = false}) {
    using((Arena arena) => _handleFfiResult(_deleteMessage(
      targetId.toNativeUtf8(allocator: arena),
      msgId.toNativeUtf8(allocator: arena),
      isGroup,
      deletedByAdmin,
    ), context: "Delete Message"));
  }

  void editMessage(String targetId, String msgId, String newContent, bool isGroup) {
    using((Arena arena) => _handleFfiResult(_editMessage(
      targetId.toNativeUtf8(allocator: arena),
      msgId.toNativeUtf8(allocator: arena),
      newContent.toNativeUtf8(allocator: arena),
      isGroup,
    ), context: "Edit Message"));
  }

  void sendReaction(String targetId, String msgId, String emoji, bool isGroup) {
    using((Arena arena) => _handleFfiResult(_sendReaction(
      targetId.toNativeUtf8(allocator: arena),
      msgId.toNativeUtf8(allocator: arena),
      emoji.toNativeUtf8(allocator: arena),
      isGroup
    ), context: "Send Reaction"));
  }

  List<dynamic> getMessageReactions(String msgId) {
    final res = using((Arena arena) => _getReactions(msgId.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.cast<Uint8>().asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  // ── Elevated Messages ──────────────────────────────────────────────

  void elevateMessage(String chatId, String msgId, String content, String senderId, bool isMe) {
    using((Arena arena) => _handleFfiResult(_elevateMessage(
      chatId.toNativeUtf8(allocator: arena),
      msgId.toNativeUtf8(allocator: arena),
      content.toNativeUtf8(allocator: arena),
      senderId.toNativeUtf8(allocator: arena),
      isMe,
    ), context: "Elevate Message"));
  }

  void unelevateMessage(String chatId, String msgId) {
    using((Arena arena) => _handleFfiResult(_unelevateMessage(
      chatId.toNativeUtf8(allocator: arena),
      msgId.toNativeUtf8(allocator: arena),
    ), context: "Unelevate Message"));
  }

  List<dynamic> getElevatedMessages(String chatId) {
    final res = using((Arena arena) => _getElevatedMessages(chatId.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.cast<Uint8>().asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  bool isMessageElevated(String chatId, String msgId) {
    final res = using((Arena arena) => _isMessageElevated(
      chatId.toNativeUtf8(allocator: arena),
      msgId.toNativeUtf8(allocator: arena),
    ));
    try {
      if (res.code != 0) return false;
      final data = utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
      return data == '1';
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  // ── Optimized Last Message Queries ──────────────────────────────────

  Map<String, dynamic>? getLastMessage(String peerId) {
    final res = using((Arena arena) => _getLastMessage(peerId.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0 || res.len == 0) return null;
      final data = utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
      if (data == 'null') return null;
      return json.decode(data) as Map<String, dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  Map<String, dynamic>? getLastGroupMessage(String groupId) {
    final res = using((Arena arena) => _getLastGroupMessage(groupId.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0 || res.len == 0) return null;
      final data = utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
      if (data == 'null') return null;
      return json.decode(data) as Map<String, dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  /// Batch: get last message for ALL contacts in one FFI call.
  Map<String, dynamic> getLastMessagesAll() {
    final res = _getLastMessagesAll();
    try {
      if (res.code != 0 || res.len == 0) return {};
      final data = utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
      return json.decode(data) as Map<String, dynamic>;
    } catch (_) {
      return {};
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  /// Batch: get last message for ALL groups in one FFI call.
  Map<String, dynamic> getLastGroupMessagesAll() {
    final res = _getLastGroupMessagesAll();
    try {
      if (res.code != 0 || res.len == 0) return {};
      final data = utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
      return json.decode(data) as Map<String, dynamic>;
    } catch (_) {
      return {};
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  // ── Daily Rewards ──────────────────────────────────────────────

  Map<String, dynamic>? getDailyRewardStatus() {
    final res = _dailyRewardGetStatus();
    try {
      if (res.code != 0 || res.len == 0) return null;
      final data = utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
      if (data == '{}' || data.isEmpty) return null;
      return json.decode(data) as Map<String, dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  List<dynamic> getDailyRewardHistory(int days) {
    final res = _dailyRewardGetHistory(days);
    try {
      if (res.code != 0 || res.len == 0) return [];
      final data = utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
      return json.decode(data) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  bool recordDailyActivity(Map<String, dynamic> event) {
    final jsonStr = json.encode(event);
    final res = using((Arena arena) => _dailyRewardRecordActivity(
      jsonStr.toNativeUtf8(allocator: arena).cast<Uint8>(),
      jsonStr.length,
    ));
    try {
      if (res.code != 0) return false;
      final data = utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
      return data == '1';
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void updateDailyRewardWeights(Map<String, dynamic> weights) {
    final jsonStr = json.encode(weights);
    using((Arena arena) => _handleFfiResult(
      _dailyRewardUpdateWeights(jsonStr.toNativeUtf8(allocator: arena).cast<Uint8>(), jsonStr.length),
      context: "Update Daily Reward Weights",
    ));
  }

  void updateDailyRewardAntiGaming(Map<String, dynamic> config) {
    final jsonStr = json.encode(config);
    using((Arena arena) => _handleFfiResult(
      _dailyRewardUpdateAntiGaming(jsonStr.toNativeUtf8(allocator: arena).cast<Uint8>(), jsonStr.length),
      context: "Update Daily Reward Anti-Gaming",
    ));
  }

  Map<String, dynamic>? getDailyRewardRealtimeEarnings() {
    final res = _dailyRewardGetRealtimeEarnings();
    try {
      if (res.code != 0 || res.len == 0) return null;
      final data = utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
      if (data == '{}' || data.isEmpty) return null;
      return json.decode(data) as Map<String, dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  /// Returns the current daily reward state as a fixed-width FFI struct.
  /// Returns null if the native symbol is unavailable or the engine is not running.
  /// No heap allocations cross the FFI boundary — safe for direct UI consumption.
  FFIDailyState? getRewardsState() {
    final fn = _getRewardsState;
    if (fn == null) return null;
    return fn();
  }

  void claimHandle(String handle) {
    var h = handle.trim();
    if (!h.startsWith("i@")) {
      h = "i@$h";
    }
    using((Arena arena) => _handleFfiResult(_claimHandle(h.toNativeUtf8(allocator: arena)), context: "Claim Handle"));
  }

  Map<String, dynamic> getHandleStatus(String handle) {
    var h = handle.trim();
    if (!h.startsWith("i@")) {
      h = "i@$h";
    }
    final res = using((Arena arena) => _getHandleStatus(h.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return {};
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as Map<String, dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  /// Returns the local user's verified handle (immutable once set). Empty string if none.
  String getLocalHandle() {
    final res = _getLocalHandle();
    try {
      if (res.code != 0 || res.len == 0) return '';
      return utf8.decode(res.data.cast<Uint8>().asTypedList(res.len));
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  /// Checks if a handle is permanently claimed (verified) by any peer.
  bool isHandleClaimed(String handle) {
    var h = handle.trim();
    if (!h.startsWith("i@")) h = "i@$h";
    final res = using((Arena arena) => _isHandleClaimed(h.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0 || res.len == 0) return false;
      return res.data.cast<Uint8>()[0] == 1;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  Map<String, dynamic> getProfile() {
    final res = _getProfile();
    try {
      if (res.code != 0) return {};
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as Map<String, dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void setProfile(String? name, String? handle, String? avatar, int privacyMode) {
    using((Arena arena) {
      _handleFfiResult(
        _setProfile(
          name?.toNativeUtf8(allocator: arena) ?? nullptr,
          handle?.toNativeUtf8(allocator: arena) ?? nullptr,
          avatar?.toNativeUtf8(allocator: arena) ?? nullptr,
          privacyMode,
        ),
        context: "Set Profile",
      );
    });
  }

  void sendFile(String peerId, String filePath, [String? groupId]) {
    using((Arena arena) {
      _handleFfiResult(
        _sendFile(
          peerId.toNativeUtf8(allocator: arena),
          filePath.toNativeUtf8(allocator: arena),
          (groupId ?? "").toNativeUtf8(allocator: arena),
        ),
        context: "Send File",
      );
    });
  }

  void cancelFileTransfer(String transferId) {
    using((Arena arena) {
      _handleFfiResult(
        _cancelFileTransfer(
          transferId.toNativeUtf8(allocator: arena),
        ),
        context: "Cancel File Transfer",
      );
    });
  }

  Future<String> claimRewards() async {
    final comp = Completer<String>();
    final cb = NativeCallable<NativeRewardCallback>.listener((int status, Pointer<Utf8> sigPtr) {
      if (status == 0) {
        final sig = sigPtr.toDartString();
        _freeString(sigPtr); 
        comp.complete(sig);
      } else {
        final err = sigPtr.toDartString();
        _freeString(sigPtr); 
        comp.completeError(Exception("Claim error: $err"));
      }
    });
    _claimRewardsAsync(cb.nativeFunction);
    final sig = await comp.future; cb.close();
    return sig;
  }

  void freeBinary(Pointer<Uint8> ptr, int len) => _freeBinary(ptr, len);

  String computeFileHash(String filePath) {
    return using((Arena arena) {
      final res = _computeFileHash(filePath.toNativeUtf8(allocator: arena));
      if (res.code != 0) {
        String msg = "Unknown error";
        if (res.data.address != 0) {
          msg = utf8.decode(res.data.asTypedList(res.len));
          _freeBinary(res.data, res.len);
        }
        throw Exception("Failed to compute file hash: $msg");
      }
      try {
        return utf8.decode(res.data.asTypedList(res.len));
      } finally {
        _freeBinary(res.data, res.len);
      }
    });
  }

  void _handleFfiResult(FfiResult result, {String context = "Rust Core"}) {
    if (result.code != 0) {
      String msg = "Unknown error";
      if (result.data.address != 0 && result.len > 0) {
        msg = utf8.decode(result.data.asTypedList(result.len));
        _freeBinary(result.data, result.len);
      }
      debugPrint('❌ $context Error (${result.code}): $msg');
      throw Exception('$context Error (${result.code}): $msg');
    } else {
      // Free any data returned on success path (defensive)
      if (result.data.address != 0 && result.len > 0) {
        _freeBinary(result.data, result.len);
      }
      debugPrint('✅ $context: Success');
    }
  }

  void noteCreate(String id, String title, String content, String tags, [String? imagePath]) {
    using((Arena arena) {
      _handleFfiResult(_notesCreate(
        id.toNativeUtf8(allocator: arena), title.toNativeUtf8(allocator: arena),
        content.toNativeUtf8(allocator: arena), tags.toNativeUtf8(allocator: arena),
        (imagePath ?? '').toNativeUtf8(allocator: arena),
      ), context: "Note Create");
    });
  }

  void noteUpdate(String id, String title, String content, String tags, [String? imagePath]) {
    using((Arena arena) {
      _handleFfiResult(_notesUpdate(
        id.toNativeUtf8(allocator: arena), title.toNativeUtf8(allocator: arena),
        content.toNativeUtf8(allocator: arena), tags.toNativeUtf8(allocator: arena),
        (imagePath ?? '').toNativeUtf8(allocator: arena),
      ), context: "Note Update");
    });
  }

  void noteDelete(String id) {
    using((Arena arena) => _handleFfiResult(_notesDelete(id.toNativeUtf8(allocator: arena)), context: "Note Delete"));
  }

  Map<String, dynamic>? noteGet(String id) {
    final res = using((Arena arena) => _notesGet(id.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return null;
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as Map<String, dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  List<dynamic> notesGetAll() {
    final res = _notesGetAll();
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  List<dynamic> notesSearch(String query) {
    final res = using((Arena arena) => _notesSearch(query.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  int noteSaveVersion(String noteId, String title, String content, String tags) {
    final res = using((Arena arena) => _notesSaveVersion(
      noteId.toNativeUtf8(allocator: arena), title.toNativeUtf8(allocator: arena),
      content.toNativeUtf8(allocator: arena), tags.toNativeUtf8(allocator: arena),
    ));
    try {
      if (res.code != 0) return 0;
      return int.tryParse(utf8.decode(res.data.asTypedList(res.len))) ?? 0;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  List<dynamic> noteGetVersions(String noteId) {
    final res = using((Arena arena) => _notesGetVersions(noteId.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  // ==================== CALL HISTORY ====================

  late IntrovertCallHistoryLogDart _callHistoryLog;
  late IntrovertCallHistoryGetDart _callHistoryGet;
  late IntrovertCallHistoryCountDart _callHistoryCount;
  late IntrovertSearchMessagesDart _searchMessages;
  late IntrovertSearchGroupMessagesDart _searchGroupMessages;
  late IntrovertSendTypingStartDart _sendTypingStart;
  late IntrovertSendTypingStopDart _sendTypingStop;
  late IntrovertGetLastSeenDart _getLastSeen;

  void callHistoryLog(String peerId, String callType, int mediaType, int durationSeconds, bool isIncoming) {
    using((Arena arena) => _handleFfiResult(_callHistoryLog(
      peerId.toNativeUtf8(allocator: arena), callType.toNativeUtf8(allocator: arena),
      mediaType, durationSeconds, isIncoming,
    ), context: "Call History Log"));
  }

  List<dynamic> callHistoryGet([int limit = 50]) {
    final res = _callHistoryGet(limit);
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  int callHistoryCount() {
    final res = _callHistoryCount();
    try {
      if (res.code != 0) return 0;
      return int.tryParse(utf8.decode(res.data.asTypedList(res.len))) ?? 0;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  // ==================== MESSAGE SEARCH ====================

  List<dynamic> searchMessages(String peerId, String query) {
    final res = using((Arena arena) => _searchMessages(
      peerId.toNativeUtf8(allocator: arena), query.toNativeUtf8(allocator: arena),
    ));
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  List<dynamic> searchGroupMessages(String groupId, String query) {
    final res = using((Arena arena) => _searchGroupMessages(
      groupId.toNativeUtf8(allocator: arena), query.toNativeUtf8(allocator: arena),
    ));
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  // ==================== TYPING INDICATOR & LAST SEEN ====================

  void sendTypingStart(String peerId) {
    using((Arena arena) => _handleFfiResult(_sendTypingStart(peerId.toNativeUtf8(allocator: arena)), context: "Send Typing Start"));
  }

  void sendTypingStop(String peerId) {
    using((Arena arena) => _handleFfiResult(_sendTypingStop(peerId.toNativeUtf8(allocator: arena)), context: "Send Typing Stop"));
  }

  int getLastSeen(String peerId) {
    final res = using((Arena arena) => _getLastSeen(peerId.toNativeUtf8(allocator: arena)));
    try {
      if (res.code != 0) return 0;
      return int.tryParse(utf8.decode(res.data.asTypedList(res.len))) ?? 0;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  List<dynamic> getRbns() {
    final res = _getRbns();
    try {
      if (res.code != 0) return [];
      return json.decode(utf8.decode(res.data.asTypedList(res.len))) as List<dynamic>;
    } finally {
      if (res.len > 0) _freeBinary(res.data, res.len);
    }
  }

  void testRbn(String address) {
    using((Arena arena) => _handleFfiResult(_testRbn(address.toNativeUtf8(allocator: arena)), context: "Test RBN Connection"));
  }

  // --- Disclaimer / Terms of Use ---

  /// Checks if the disclaimer has been accepted.
  /// Requires dbPath and seed because this runs BEFORE the engine starts.
  bool isDisclaimerAccepted(String dbPath, Uint8List seed) {
    return using((Arena arena) {
      final dbPtr = dbPath.toNativeUtf8(allocator: arena);
      final seedPtr = arena.allocate<Uint8>(32);
      seedPtr.asTypedList(32).setAll(0, seed);
      return _disclaimerIsAccepted(dbPtr, seedPtr) == 1;
    });
  }

  /// Sets the disclaimer acceptance status.
  void setDisclaimerAccepted(String dbPath, Uint8List seed, bool accepted) {
    using((Arena arena) {
      final dbPtr = dbPath.toNativeUtf8(allocator: arena);
      final seedPtr = arena.allocate<Uint8>(32);
      seedPtr.asTypedList(32).setAll(0, seed);
      _handleFfiResult(_disclaimerSetAccepted(dbPtr, seedPtr, accepted), context: "Set Disclaimer Accepted");
    });
  }
}

// ==================== CALL HISTORY TYPEDEFS ====================

typedef IntrovertCallHistoryLogC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> callType, Int32 mediaType, Int32 durationSeconds, Bool isIncoming);
typedef IntrovertCallHistoryLogDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> callType, int mediaType, int durationSeconds, bool isIncoming);
typedef IntrovertCallHistoryGetC = FfiResult Function(Int32 limit);
typedef IntrovertCallHistoryGetDart = FfiResult Function(int limit);
typedef IntrovertCallHistoryCountC = FfiResult Function();
typedef IntrovertCallHistoryCountDart = FfiResult Function();

typedef IntrovertSearchMessagesC = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> query);
typedef IntrovertSearchMessagesDart = FfiResult Function(Pointer<Utf8> peerId, Pointer<Utf8> query);
typedef IntrovertSearchGroupMessagesC = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> query);
typedef IntrovertSearchGroupMessagesDart = FfiResult Function(Pointer<Utf8> groupId, Pointer<Utf8> query);

typedef IntrovertSendTypingStartC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertSendTypingStartDart = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertSendTypingStopC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertSendTypingStopDart = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertGetLastSeenC = FfiResult Function(Pointer<Utf8> peerId);
typedef IntrovertGetLastSeenDart = FfiResult Function(Pointer<Utf8> peerId);

typedef IntrovertNetworkGetRbnsC = FfiResult Function();
typedef IntrovertNetworkGetRbnsDart = FfiResult Function();
typedef IntrovertNetworkTestRbnC = FfiResult Function(Pointer<Utf8> address);
typedef IntrovertNetworkTestRbnDart = FfiResult Function(Pointer<Utf8> address);

// Disclaimer / Terms of Use
typedef IntrovertDisclaimerIsAcceptedC = Int32 Function(Pointer<Utf8> dbPath, Pointer<Uint8> seed);
typedef IntrovertDisclaimerIsAcceptedDart = int Function(Pointer<Utf8> dbPath, Pointer<Uint8> seed);
typedef IntrovertDisclaimerSetAcceptedC = FfiResult Function(Pointer<Utf8> dbPath, Pointer<Uint8> seed, Bool accepted);
typedef IntrovertDisclaimerSetAcceptedDart = FfiResult Function(Pointer<Utf8> dbPath, Pointer<Uint8> seed, bool accepted);
