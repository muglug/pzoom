<?php
/** @param iterable<string,object> $_p */
function accepts(iterable $_p): void {}

/** @return array<int,int> */
function arr() { return [1]; }

accepts(arr());
