/**
 * WebAuthn for the passkeys of farglass (F034).
 *
 * farglassd sends the options of WebAuthn as JSON, with each binary value in base64url,
 * and expects the credential in the same form. This module converts both ways.
 */

/** Returns the bytes of a base64url text. */
export function fromBase64url(text: string): ArrayBuffer {
  const base64 = text.replaceAll("-", "+").replaceAll("_", "/").padEnd(Math.ceil(text.length / 4) * 4, "=");
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
}

/** Returns the base64url text of bytes, with no padding. */
export function toBase64url(buffer: ArrayBuffer | ArrayBufferView): string {
  const bytes = buffer instanceof ArrayBuffer ? new Uint8Array(buffer) : new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

type Json = Record<string, unknown>;

const credentials = (list: unknown): PublicKeyCredentialDescriptor[] =>
  Array.isArray(list) ? list.map((each: Json) => ({ ...(each as unknown as PublicKeyCredentialDescriptor), id: fromBase64url(String(each["id"])) })) : [];

/** Returns the options of `navigator.credentials.create` from the JSON of farglassd. */
export function creationOptions(json: unknown): PublicKeyCredentialCreationOptions {
  const options = json as Json;
  const user = options["user"] as Json;
  return {
    ...(options as unknown as PublicKeyCredentialCreationOptions),
    challenge: fromBase64url(String(options["challenge"])),
    user: { ...(user as unknown as PublicKeyCredentialUserEntity), id: fromBase64url(String(user["id"])) },
    excludeCredentials: credentials(options["excludeCredentials"]),
  };
}

/** Returns the options of `navigator.credentials.get` from the JSON of farglassd. */
export function requestOptions(json: unknown): PublicKeyCredentialRequestOptions {
  const options = json as Json;
  return {
    ...(options as unknown as PublicKeyCredentialRequestOptions),
    challenge: fromBase64url(String(options["challenge"])),
    allowCredentials: credentials(options["allowCredentials"]),
  };
}

/** Returns the JSON of a credential for farglassd: each binary value in base64url. */
export function credentialJson(credential: PublicKeyCredential): Json {
  const response = credential.response;
  const out: Json = { clientDataJSON: toBase64url(response.clientDataJSON) };
  if ("attestationObject" in response) {
    const attestation = response as AuthenticatorAttestationResponse;
    out["attestationObject"] = toBase64url(attestation.attestationObject);
    const transports = attestation.getTransports?.();
    if (transports) out["transports"] = transports;
  } else {
    const assertion = response as AuthenticatorAssertionResponse;
    out["authenticatorData"] = toBase64url(assertion.authenticatorData);
    out["signature"] = toBase64url(assertion.signature);
    if (assertion.userHandle) out["userHandle"] = toBase64url(assertion.userHandle);
  }
  return {
    id: credential.id,
    rawId: toBase64url(credential.rawId),
    type: credential.type,
    response: out,
    extensions: credential.getClientExtensionResults(),
  };
}

/** Reports whether the browser has WebAuthn on this page. It needs a secure context: HTTPS with a valid certificate, or localhost. */
export function hasPasskeys(): boolean {
  return typeof window !== "undefined" && window.isSecureContext && "PublicKeyCredential" in window && !!navigator.credentials;
}

/** Returns the text for the user of a WebAuthn error. */
export function passkeyError(error: unknown): string {
  const name = error instanceof DOMException ? error.name : "";
  if (name === "NotAllowedError") return "The passkey was not used: the request was cancelled or timed out.";
  if (name === "InvalidStateError") return "This passkey is already registered.";
  if (name === "SecurityError") return "The browser refused the passkey for this address. A passkey needs a host name and a valid certificate.";
  return error instanceof Error ? error.message : String(error);
}
