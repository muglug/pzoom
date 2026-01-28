<?php
function takeVariadicInts(int ...$inputs): void {}

$a = [3, 5, 7];
takeVariadicInts(...$a);
