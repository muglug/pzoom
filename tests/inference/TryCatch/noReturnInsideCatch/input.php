<?php
/**
 * @return never-returns
 */
function example() : void {
    throw new Exception();
}

try {
    $str = "a";
} catch (Exception $e) {
    example();
}
ord($str);
