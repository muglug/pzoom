<?php

/** @property int $test1 */
class a {
    public function __get(string $name)
    {
        return 0;
    }
}

/** @var a&object{test2: "lmao"} */
$r = null;

$test1 = $r->test1;
$test2 = $r->test2;
