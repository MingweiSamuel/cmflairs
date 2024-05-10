<script lang="ts">
  const formatter = new Intl.NumberFormat();

  export let champion: number = 143;
  export let points: number = 123_456;
  export let level: number = 7;
  export let name: string = 'Zyra';
</script>

<div class="champ-badge" title={name}>
  <div class="banner" data-mastery={level}></div>
  <div
    class="champ"
    style="background-image: url(&quot;https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/champion-icons/{champion}.png&quot;);"
  ></div>
  <div class="circle"></div>
  <div class="mastery" data-mastery={level}></div>
  <div class="name">{name}</div>
  <div class="points">{formatter.format(points)}</div>
</div>

<style lang="scss">
  .champ-badge {
    position: relative;
    // width: 100%;
    width: 100px;
    padding-bottom: 125%;
    margin-bottom: 4.8em;

    .circle {
      background-image: url('/img/champ_circle.png');
      background-size: 108.3333%;
      width: 100%;
      padding-bottom: 100%;
      background-position: center;
      background-repeat: no-repeat;
      position: absolute;
    }

    .champ {
      background-size: 107%;
      background-position: center;
      background-repeat: no-repeat;
      border-radius: 100%;
      position: absolute;
      width: 77%;
      padding-bottom: 77%;
      top: 8.75%;
      left: 11.5%;
      box-shadow: 0 0 1em 0.8em black;
    }

    .mastery {
      background-image: url('/img/champ_masteries.png');
      background-size: 100%;
      width: 55%;
      padding-bottom: 55%;
      background-repeat: no-repeat;
      position: absolute;
      left: 22.5%;
      top: 57%;
    }

    .banner {
      background-image: url('/img/champ_banners.png');
      background-size: 100%;
      width: 75%;
      padding-bottom: 75%;
      background-repeat: no-repeat;
      position: absolute;
      left: 12.5%;
      top: 40%;
      box-shadow: 0 -0.8em 0.8em -0.1em black;
    }

    .banner[data-mastery='0'] {
      box-shadow: none;
    }

    .mastery,
    .banner {
      @for $i from 1 through 7 {
        &[data-mastery='#{$i}'] {
          background-position-y: calc($i / 7 * 100%);
        }
      }
    }

    .name {
      position: absolute;
      width: 100%;
      bottom: -1.5em;
      font-size: 200%;
      height: 2em;
      line-height: 2em;
    }

    .points {
      position: absolute;
      width: 100%;
      bottom: -3em;
      font-size: 150%;
      height: 1.5em;
      line-height: 1.5em;
    }
  }
</style>
