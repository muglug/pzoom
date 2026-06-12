<?php
final class MethodId {}
final class CTC {
    /** @return null|'not-callable'|MethodId */
    public static function getId(int $x): null|string|MethodId {
        if ($x === 0) { return null; }
        if ($x === 1) { return 'not-callable'; }
        return new MethodId();
    }
}
function takesId(MethodId $id): void { echo get_class($id); }

function f(int $x): void {
    $potential = CTC::getId($x);
    if ($potential && $potential !== 'not-callable') {
        takesId($potential);
    }
}
