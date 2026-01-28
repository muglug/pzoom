<?php
interface A {}
class B implements A {}

/**
 * @param  A      $a
 * @return void
 */
function qux(A $a) { }

qux(new B());
