<?php
trait T {
  /** @var string **/
  public $foo;

  public function __construct() {
    $this->foo = "hello";
  }
}

class A {
    use T;
}
