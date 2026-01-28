<?php
function foo(Traversable $t): void {
    if ($t instanceof IteratorAggregate) {
        $a = $t->getIterator();
        $t = $a;
    }

    if (!$t instanceof Iterator) {
        return;
    }

    if (rand(0, 1) && rand(0, 1)) {
        $t->next();
    }
}