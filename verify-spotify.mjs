// One-off check: does this Spotify app actually have Web API access?
//
//   node verify-spotify.mjs <client_id> <client_secret>
//
// Uses the client-credentials flow purely as a diagnostic — it is the shortest
// path to a definitive answer and needs no browser. The player itself uses
// PKCE and never sees the secret, so you can rotate or delete the secret in
// the dashboard as soon as this has run.

const [clientId, clientSecret] = process.argv.slice(2);

if (!clientId || !clientSecret) {
  console.error("usage: node verify-spotify.mjs <client_id> <client_secret>");
  process.exit(1);
}

const basic = Buffer.from(`${clientId}:${clientSecret}`).toString("base64");

const tokenRes = await fetch("https://accounts.spotify.com/api/token", {
  method: "POST",
  headers: {
    Authorization: `Basic ${basic}`,
    "Content-Type": "application/x-www-form-urlencoded",
  },
  body: "grant_type=client_credentials",
});

const tokenBody = await tokenRes.text();

if (!tokenRes.ok) {
  console.log(`TOKEN FAILED  HTTP ${tokenRes.status}`);
  console.log(tokenBody.slice(0, 300));
  console.log(
    "\ninvalid_client => wrong id/secret." +
      "\nanything else  => the app itself is not permitted to use the Web API.",
  );
  process.exit(1);
}

const token = JSON.parse(tokenBody).access_token;
console.log("token OK");

// Search is the only endpoint the player needs, to turn SMTC's
// title/artist strings into a Spotify track id.
const q = 'track:"Pure" artist:"Rex Orange County"';
const searchRes = await fetch(
  `https://api.spotify.com/v1/search?${new URLSearchParams({ q, type: "track", limit: "1" })}`,
  { headers: { Authorization: `Bearer ${token}` } },
);

if (!searchRes.ok) {
  console.log(`SEARCH FAILED  HTTP ${searchRes.status}`);
  console.log((await searchRes.text()).slice(0, 300));
  console.log("\n403 here means the account/app is barred from the Web API.");
  process.exit(1);
}

const hit = (await searchRes.json())?.tracks?.items?.[0];
console.log(
  hit
    ? `search OK -> "${hit.name}" by ${hit.artists[0].name} (id ${hit.id})`
    : "search returned no results (unexpected, but the API itself works)",
);
console.log("\nWEB API ACCESS CONFIRMED — the greyed checkbox is cosmetic.");
