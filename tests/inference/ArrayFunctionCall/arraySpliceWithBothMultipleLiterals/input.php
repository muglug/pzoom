<?php
$a = array( "hello" );
/** @var 1|2|0 **/
$b = 1;
/** @var 4|5 **/
$c = 4;
$_d = array_splice( $a, $b, $c );
