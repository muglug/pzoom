<?php
/** @return Generator<int, string, DateTimeInterface, void> */
function g(): Generator {
    yield 1 => "string";
}
g()->send(1);
                
