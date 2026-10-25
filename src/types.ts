export interface Finding {
  level: "ERROR" | "WARN" | "INFO" | "OK";
  pack: string;
  msg: string;
}

export interface ConflictRow {
  level: "ERROR" | "WARN" | "INFO";
  chain: string[];
  table: string;
  note: string;
}

export interface SimpleItem {
  level: "ERROR" | "WARN" | "INFO" | "OK";
  msg: string;
}

export interface ScanReport {
  scannedAt: string;
  gameDir: string;
  packsTotal: number;
  packsOk: number;
  packsBad: number;
  modsEnabled: number;
  errors: number;
  warnings: number;
  findings: Finding[];
  conflicts: ConflictRow[];
  tech: SimpleItem[];
}
