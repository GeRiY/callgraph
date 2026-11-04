<template>
  <div class="box">
    <button v-if="enabled" @click="save">Save</button>
    <p v-else>{{ $t('sample.disabled') }}</p>
    <ChildBox :value="total" @change="onChange" />
  </div>
</template>

<script>
import ChildBox from './ChildBox.vue';
import { helper } from './helpers';

export default {
  name: 'OptionsApi',
  components: { ChildBox },
  props: { enabled: { type: Boolean, required: true }, size: Number },
  emits: ['saved'],
  data() {
    return { total: 0 };
  },
  computed: {
    doubled() {
      return this.total * 2;
    },
  },
  watch: {
    total(value) {
      this.log(value);
    },
  },
  methods: {
    save() {
      this.$emit('saved', helper(this.total));
    },
    onChange(v) {
      this.total = v;
    },
    log(value) {
      console.log('total', value);
    },
  },
};
</script>
