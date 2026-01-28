<?php
/** @var int<0, 5> */
$a = 2;
/** @var int<6, 10> */
$b = 9;

/**
 * @param int<0, 5> $_a
 * @param int<6, 10> $_b
 */
function foo(int $_a, int $_b): void {}

foo(...[$a, $b]);
                
