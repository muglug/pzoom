<?php
class Foo {}
class Bar {}

interface I {
  /** @return array<int, Foo> */
  public function getFoos() : array;
}

class A implements I {
    public function getFoos() : array {
        return [new Bar()];
    }
}
