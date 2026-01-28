<?php
/** @template T as mixed */
interface I {
    /**
     * @param  T $v
     * @return array<string,T>
     */
    public function indexById($v): array;
}

/** @template-implements I<string> */
class C implements I {
    public function indexById($v): array {
      return [$v => $v];
    }
}