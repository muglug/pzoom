<?php
/** @param non-empty-list<mixed> $_list */
function foobar(array $_list): void {}

$list = random_int(0, 1) ? [] : ["foobar"];

foobar($list);
                
