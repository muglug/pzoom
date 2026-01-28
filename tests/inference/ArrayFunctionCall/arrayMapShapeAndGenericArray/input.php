<?php
/** @return string[] */
function getLine(): array { return ["a", "b"]; }

$line = getLine();

if (empty($line[0])) { // converts array<string> to array{0:string}<string>
    throw new InvalidArgumentException;
}

$line = array_map( // should not destroy <string> part
    function($val) { return (int)$val; },
    $line
);
                
