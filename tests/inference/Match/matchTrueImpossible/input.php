<?php
$foo = new \stdClass();
$a = match (true) {
    $foo instanceof \stdClass => 1,
    $foo instanceof \Exception => 1,
};
