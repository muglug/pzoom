<?php
class A {
  /** @var int */
  private $foo;

  public function __construct(string $foo) {
    $this->foo = 5;
  }

  private function bar() : void {}
}

class B extends A {
  /** @var string */
  private $foo;

  public function __construct(string $foo) {
    $this->foo = $foo;
    parent::__construct($foo);
  }
}
