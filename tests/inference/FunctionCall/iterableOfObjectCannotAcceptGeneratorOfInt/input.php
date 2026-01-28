<?php
/** @param iterable<string,object> $_p */
function accepts(iterable $_p): void {}

/** @return Generator<int,int,mixed,void> */
function generator() { yield 1; }

accepts(generator());
