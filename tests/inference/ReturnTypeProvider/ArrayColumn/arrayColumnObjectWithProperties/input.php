<?php
/**
 * @param object{id: int} $o
 * @return non-empty-list<int>
 */
function f(object $o): array {
    return array_column([$o], "id");
}
            
