<?php
class A {}
interface I {
  /** @return A */
  public function foo();
}

class B extends A implements I {
  /** @return static */
  public function foo() {
    return $this;
  }
}
