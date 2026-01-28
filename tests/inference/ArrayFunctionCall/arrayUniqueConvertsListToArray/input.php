<?php
/** @param non-empty-list<object> $input */
function takes_non_empty_list(array $input): void {}

takes_non_empty_list(array_unique([(object)[]]));
                
