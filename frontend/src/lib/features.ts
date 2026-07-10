/**
 * Release-channel feature gates (ADR 0014).
 *
 * Physical rotor control is disabled in the alpha channel: the serial rotor
 * stack is code-complete but has never been verified against a real G-5500,
 * and the audit hardening for the serial path is still open. The backend
 * enforces the same gate at the IPC boundary (`commands/serial_rotor.rs`),
 * so this flag only controls what the UI offers. Rotor *analysis* surfaces
 * (Operator Brief, pass feasibility, the rotor profile in Settings) are pure
 * computation and stay available.
 */
export const ROTOR_ENABLED: boolean = false;
