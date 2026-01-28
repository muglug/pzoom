<?php
function testInt(int $var): void {

}

function testString(string $var): void {

}

$a = rand(0, 10) ? 1 : "two";

switch (gettype($a)) {
    case "string":
        testString($a);
        break;

    case "integer":
        testInt($a);
        break;
}
