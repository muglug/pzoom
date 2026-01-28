<?php
class U {
    public function takes(self $u) : bool {
        return true;
    }
}

function bar(?U $a, ?U $b) : void {
    if ($a === null
        || ($b !== null && $a->takes($b))
        || $b === null
    ) {}
}