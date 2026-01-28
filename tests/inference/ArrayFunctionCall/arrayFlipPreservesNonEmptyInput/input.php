<?php
/** @param non-empty-array<string, int> $input */
function takes_non_empty_array(array $input): void {}

$array = ["hi", "there"];
$flipped = array_flip($array);

takes_non_empty_array($flipped);
                
