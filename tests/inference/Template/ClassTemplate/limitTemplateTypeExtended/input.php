<?php

/**
 * @template T as object
 */
abstract class A {}

/**
 * @extends A<stdClass>
 */
class AChild extends A {}

function takesA(A $a) : void {}

$child = new AChild();
takesA($child);
