<?php
final class Test {
    public function __construct(public int $id, public string $name) {}
}

$test = new Test(1, "ame");
echo $test->id;
echo $test->name;
