<?php
interface I1 {}
interface I2 {}

/**
 * @template T as I1&I2
 * @param T $b
 */
function templatedBar(I2 $b) : void {}