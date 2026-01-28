<?php

class Subclass extends DateTimeImmutable
{
}

$datetime = new Subclass();
$format = $datetime->modify("+1 day")->format("Y-m-d");
