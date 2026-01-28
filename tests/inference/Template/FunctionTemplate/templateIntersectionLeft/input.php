<?php
interface I1 {}
interface I2 {}

/**
 * @template T as I1&I2
 * @param T $a
 */
function templatedBar(I1 $a) : void {}