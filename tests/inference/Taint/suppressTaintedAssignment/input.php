<?php
$b = $_GET["x"];

/**
 * @psalm-suppress TaintedInput
 */
$a = $b;


echo $a;
