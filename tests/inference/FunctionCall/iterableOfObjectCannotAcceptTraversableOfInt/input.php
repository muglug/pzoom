<?php
/** @param iterable<string,object> $_p */
function accepts(iterable $_p): void {}

/** @return Traversable<int,int> */
function traversable() { yield 1; }

accepts(traversable());
