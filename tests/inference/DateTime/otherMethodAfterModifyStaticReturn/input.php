<?php

class Subclass extends DateTimeImmutable
{
}

$datetime = new Subclass();
$mod = $datetime->modify("+1 day")->setTime(0, 0);
