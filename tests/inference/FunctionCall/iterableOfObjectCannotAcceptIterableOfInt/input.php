<?php
/** @param iterable<string,object> $_p */
function accepts(iterable $_p): void {}

/** @return iterable<int,int> */
function iterable() { yield 1; }

accepts(iterable());
