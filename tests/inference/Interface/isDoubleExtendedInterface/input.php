<?php
interface A {}
interface B extends A {}
class C implements B {}

/**
 * @param  A      $a
 * @return void
 */
function qux(A $a) {
}

qux(new C());
