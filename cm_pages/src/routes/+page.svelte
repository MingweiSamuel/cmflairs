<script lang="ts">
  import { onMount } from 'svelte';
  import { replaceState } from '$app/navigation';
  import Counter from './Counter.svelte';

  const WORKER_ORIGIN = import.meta.env.VITE_WORKER_ORIGIN;

  let signinToken: string | null = null;
  onMount(async () => {
    {
      const session_token = localStorage.getItem('SESSION_TOKEN');
      if (null != session_token) {
        const resp = await fetch(`${WORKER_ORIGIN}/user/me`, {
          headers: { Authorization: `Bearer ${session_token}` }
        });
        let data = await resp.json();
        console.log(data);
      }
    }

    const url = new URL(document.location.href);
    const state = url.searchParams.get('state');
    const token = url.searchParams.get('token');
    if (null != state || null != token) {
      url.search = '';
      replaceState(url, { sess: { state, token } });
      return;
    }
    if (null != history.state.sess) {
      const { state, token } = history.state.sess;
      const oldState = localStorage.getItem('SIGNIN_TOKEN');
      if (oldState === state) {
        try {
          const resp = await fetch(`${WORKER_ORIGIN}/signin/upgrade`, {
            headers: { Authorization: `Bearer ${token}` }
          });
          const finalToken = await resp.json();
          localStorage.setItem('SESSION_TOKEN', finalToken);
          return;
        } catch {
          localStorage.removeItem('SIGNIN_TOKEN');
        }
      }
    }
    const resp = await fetch(`${WORKER_ORIGIN}/signin/anonymous`);
    signinToken = await resp.json();
    localStorage.setItem('SIGNIN_TOKEN', signinToken!);
  });
</script>

<svelte:head>
  <title>Home</title>
  <meta name="description" content="Svelte demo app" />
</svelte:head>

<section>
  <Counter />
  <form action={`${WORKER_ORIGIN}/signin/reddit`}>
    <input type="hidden" name="state" value={signinToken} />
    <input type="submit" value="Sign In With Reddit" disabled={null == signinToken} />
  </form>
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
