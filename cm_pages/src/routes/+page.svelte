<script lang="ts">
  import { onMount } from 'svelte';
  import Counter from './Counter.svelte';

  const WORKER_ORIGIN = import.meta.env.VITE_WORKER_ORIGIN;

  let signinToken: string | null = null;
  onMount(async () => {
    const url = new URL(document.location.href);
    const state = url.searchParams.get('state');
    const token = url.searchParams.get('token');
    if (null != state || null != token) {
      url.search = '';
      history.replaceState(Object.assign({}, history.state, { sess: { state, token }}), '', url);
      return;
    }
    if (null != history.state.sess) {
      const { state, token } = history.state.sess;
      const oldState = localStorage.getItem('SIGNIN_TOKEN');
      if (oldState === state) {
        const resp = await fetch(`${WORKER_ORIGIN}/signin/upgrade`, {
          headers: { Authorization: `Bearer ${token}` }
        });
        const finalToken = await resp.json();
        localStorage.setItem('SESSION_TOKEN', finalToken);
        return;
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
