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
  if (($obj->a === null) == false) {
    return $obj->a; // definitely not null
  } elseif (is_null($obj->b) == false) {
    return $obj->b;
  } else {
    throw new \InvalidArgumentException("$obj->a or $obj->b must be set");
  }
}