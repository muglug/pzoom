<?php
/**
 * @param array{id:int} $shape
 * @return non-empty-list<int>
 */
function f(array $shape): array {
    return array_column([$shape], "id");
}
            
