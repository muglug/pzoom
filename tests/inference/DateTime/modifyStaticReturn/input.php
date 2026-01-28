<?php

class Subclass extends DateTimeImmutable
{
}

$foo = new Subclass("2023-01-01 12:12:13");
$mod = $foo->modify("+7 days");
