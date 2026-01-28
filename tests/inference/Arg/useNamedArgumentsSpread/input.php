<?php
function takesArguments(string $name, int $age) : void {}

$args = ["name" => "hello", "age" => 5];
takesArguments(...$args);
