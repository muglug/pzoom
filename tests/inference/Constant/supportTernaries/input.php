<?php
const cons1 = true;

class Clazz {
    /**
     * @psalm-suppress RedundantCondition
     */
    const cons2 = (cons1) ? 1 : 0;
}

echo Clazz::cons2;
