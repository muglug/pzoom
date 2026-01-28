<?php
/** @return non-empty-array<string> */
function getStrings(): array { return ["test"]; }
/** @return non-empty-array<int> */
function getInts(): array { return [123]; }
$c = array_combine(getStrings(), getInts());
