<script lang="ts">
  import { page } from '$app/stores';
  import { afterNavigate, replaceState } from '$app/navigation';
  import ChampBadge from './ChampBadge.svelte';

  const WORKER_ORIGIN = import.meta.env.VITE_WORKER_ORIGIN;

  async function onSessionChange() {
    // See if we are signed in.
    const session_token = localStorage.getItem('SESSION_TOKEN');
    if (null != session_token) {
      try {
        const resp = await fetch(`${WORKER_ORIGIN}/user/me`, {
          headers: { Authorization: `Bearer ${session_token}` }
        });
        if (resp.ok) {
          userData = await resp.json();
          return;
        }
      } catch (e) {
        console.error(e);
      }
      localStorage.removeItem('SESSION_TOKEN');
    }

    // See if we have a transition token.
    {
      const state = $page.url.searchParams.get('state');
      const token = $page.url.searchParams.get('token');
      if (null != state || null != token) {
        const url = new URL($page.url);
        url.search = '';
        replaceState(url, {});

        const oldState = localStorage.getItem('ANONYMOUS_TOKEN');
        localStorage.removeItem('ANONYMOUS_TOKEN');
        if (oldState === state) {
          const resp = await fetch(`${WORKER_ORIGIN}/signin/upgrade`, {
            headers: { Authorization: `Bearer ${token}` }
          });
          if (resp.ok) {
            const sessionToken: string = await resp.json();
            localStorage.setItem('SESSION_TOKEN', sessionToken);
            return onSessionChange();
          }
        }
      }
    }

    // Get anonymous token.
    {
      const resp = await fetch(`${WORKER_ORIGIN}/signin/anonymous`);
      anonymousToken = await resp.json();
      localStorage.setItem('ANONYMOUS_TOKEN', anonymousToken!);
    }
  }

  async function updateSummoner(sid: number): Promise<boolean> {
    const sessionToken = localStorage.getItem('SESSION_TOKEN');
    const resp = await fetch(`${WORKER_ORIGIN}/summoner/${sid}/update`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${sessionToken}`
      }
    });
    return resp.ok;
  }

  let userData: any | null;
  let anonymousToken: string | null = null;

  afterNavigate(onSessionChange);
</script>

<svelte:head>
  <title>Home</title>
  <meta name="description" content="Svelte demo app" />
</svelte:head>

<section>
  <form action={`${WORKER_ORIGIN}/signin/reddit`}>
    <input type="hidden" name="state" value={anonymousToken} />
    <input type="submit" value="Sign In With Reddit" disabled={null == anonymousToken} />
  </form>
  {#if null != userData}
    <div style="width: 100px;">
      {#each userData.champs as { champ_id, total_points, max_level, name }}
        <ChampBadge champion={champ_id} points={total_points} level={max_level} {name} />
      {/each}
    </div>
    {#each userData.summoners as summoner}
      <div>
        {summoner.game_name}#{summoner.tag_line} ({summoner.platform})
        <button on:click={() => updateSummoner(summoner.id)}> Update </button>
      </div>
    {/each}
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    flex: 0.6;
  }
</style>
