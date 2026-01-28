<?php
/** @param non-empty-array<string, object> $input */
function takes_non_empty_array(array $input): void {}

takes_non_empty_array(array_unique(["test" => (object)[]]));

/** @param non-empty-array<int, object> $input */
function takes_non_empty_int_array(array $input): void {}

takes_non_empty_int_array(array_unique([(object)[]]));
                
