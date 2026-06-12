<?php
function foo() : array {
    return ["hello" => new stdClass, "goodbye" => new stdClass];
}

$_a = null;
$_b = null;

/**
 * @var string $_key
 * @var stdClass $_value
 */
foreach (foo() as $_key => $_value) {
    $_a = $_key;
    $_b = $_value;
}
