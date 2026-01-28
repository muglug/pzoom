<?php
/** @return Generator<int, string, DateTimeInterface, void> */
function g(): Generator {
    $date = yield 1 => "string";
    $date->format("m");
}
g()->send(new \DateTime("now"));
