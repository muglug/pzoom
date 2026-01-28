<?php
class A {
   /**
    * @var ?string
    */
   public $a = null;
   /**
    * @var ?string
    */
   public $b = null;
}
function f(A $obj): string {
  switch (true) {
    case $obj->a !== null:
      return $obj->a; // definitely not null
    case !is_null($obj->b):
      return $obj->b; // definitely not null
    default:
      throw new \InvalidArgumentException("$obj->a or $obj->b must be set");
  }
}
