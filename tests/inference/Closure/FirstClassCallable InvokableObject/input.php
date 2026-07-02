<?php
class Test {
    public function __invoke(string $param): int {
        return strlen($param);
    }
}
$test = new Test();
$closure = $test(...);
$length = $closure("test");
