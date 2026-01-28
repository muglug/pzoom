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
    case $obj->a:
      return $obj->a; // definitely not null
    case $obj->b:
      return $obj->b; // definitely not null
    default:
      throw new \InvalidArgumentException("$obj->a or $obj->b must be set");
  }
}
