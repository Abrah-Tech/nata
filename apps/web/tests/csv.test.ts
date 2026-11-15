import { describe, expect, test } from "bun:test";

import { toCsv } from "../lib/csv";

describe("toCsv", () => {
  test("joins fields with commas and rows with CRLF", () => {
    expect(toCsv([["a", "b"], ["1", "2"]])).toBe("a,b\r\n1,2\r\n");
  });

  test("quotes a field that carries a comma, a quote or a newline", () => {
    expect(toCsv([["Roti, sobek", 'Roti "spesial"', "dua\nbaris"]])).toBe(
      '"Roti, sobek","Roti ""spesial""","dua\nbaris"\r\n',
    );
  });

  test("keeps a negative amount numeric", () => {
    expect(toCsv([["-1234.56"]])).toBe("-1234.56\r\n");
  });

  test("stops a typed name from becoming a formula", () => {
    expect(toCsv([["=1+1"]])).toBe("'=1+1\r\n");
    expect(toCsv([["@SUM(A1)"]])).toBe("'@SUM(A1)\r\n");
  });

  test("quotes a field with surrounding spaces so the spacing survives", () => {
    expect(toCsv([[" Gula "]])).toBe('" Gula "\r\n');
  });

  test("an empty row is a blank line between sections", () => {
    expect(toCsv([["Total", "10"], [], ["Bahan"]])).toBe(
      "Total,10\r\n\r\nBahan\r\n",
    );
  });
});
