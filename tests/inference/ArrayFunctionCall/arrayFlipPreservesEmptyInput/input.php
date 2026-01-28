<?php
/** @param non-empty-array<string, int> $input */
function takes_non_empty_array(array $input): void {}

takes_non_empty_array(array_flip([]));
                
