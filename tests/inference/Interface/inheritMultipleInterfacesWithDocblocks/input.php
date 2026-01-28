<?php
interface I1 {
  /** @return string */
  public function foo();
}
interface I2 {
  /** @return string */
  public function bar();
}
class A implements I1, I2 {
  public function foo() {
    return "hello";
  }
  public function bar() {
    return "goodbye";
  }
}
