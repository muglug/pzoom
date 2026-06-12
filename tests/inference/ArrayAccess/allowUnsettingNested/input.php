<?php
/** @psalm-immutable */
final class test {
    public function __construct(public int $value) {}
}
$test = new test(1);
$a = [1 => $test];
unset($a[$test->value]);
