import { expect, test, type APIRequestContext } from "@playwright/test";
import { API, apiCall, signUp, upload } from "./helpers";

// Link previews: what a messenger's bot finds in the page's initial HTML.
// No browser needed; the page is fetched the way WhatsApp fetches it.

test.skip(({ isMobile }) => isMobile, "fetches HTML only; one project is enough");

const BOT = { "User-Agent": "WhatsApp/2.23.20.0 A" };

async function metaTags(request: APIRequestContext, url: string) {
  const html = await (await request.get(url, { headers: BOT })).text();
  const head = html.split("</head>")[0];
  const tags: Record<string, string> = {};
  for (const [, key, value] of head.matchAll(/<meta (?:property|name)="([^"]+)" content="([^"]*)"/g)) tags[key] = value;
  const title = head.match(/<title>([^<]*)<\/title>/)?.[1];
  return { tags, title };
}

test("a public post has title, caption, image and noindex", async ({ request, baseURL }) => {
  const alice = await signUp(request, "alice");
  const post = await upload(request, alice, "Sunset over the Elbe", "portrait");
  const { tags, title } = await metaTags(request, `${baseURL}/posts/${post}`);

  expect(title).toBe(`Post by @${alice.username} · Klar`);
  expect(tags["og:title"]).toBe(`@${alice.username} on Klar`);
  expect(tags["og:description"]).toBe("Sunset over the Elbe");
  expect(tags["og:image"]).toBe(`${API}/posts/${post}/preview-image`);
  expect(tags["og:image:width"]).toBe("640");
  expect(tags["twitter:card"]).toBe("summary_large_image");
  expect(tags["robots"]).toContain("noindex");

  const image = await request.get(tags["og:image"], { maxRedirects: 0 });
  expect(image.status()).toBe(302);
});

test("a private account's post reveals nothing", async ({ request, baseURL }) => {
  const bob = await signUp(request, "bob");
  const post = await upload(request, bob, "Private stuff");
  await apiCall(request, bob, "PATCH", "/users/me", { is_private: true });
  const { tags, title } = await metaTags(request, `${baseURL}/posts/${post}`);

  expect(title).toBe("Klar");
  expect(tags["og:title"]).toBeUndefined();
  expect(tags["og:image"]).toBeUndefined();
});

test("robots.txt lets preview bots into posts, not search engines", async ({ request, baseURL }) => {
  const robots = await (await request.get(`${baseURL}/robots.txt`)).text();
  const [star, bots] = robots.split(/User-Agent: facebookexternalhit/);
  expect(star).toContain("Disallow: /posts");
  expect(bots).toContain("Allow: /posts/");
});
