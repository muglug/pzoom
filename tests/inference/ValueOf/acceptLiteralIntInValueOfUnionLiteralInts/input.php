<?php
/**
 * @return value-of<list<0|1|2>|array{0: 3, 1: 4}>
 */
function getValue(int $i) {
    if ($i >= 0 && $i <= 4) {
        return $i;
    }
    return 0;
}
                
