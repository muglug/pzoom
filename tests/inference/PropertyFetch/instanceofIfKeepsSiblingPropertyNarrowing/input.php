<?php
class FunctionLikeStorage {
    public ?string $return_type = null;
}
class MethodStorage extends FunctionLikeStorage {
    public bool $has_docblock_return_type = false;
}
function f(FunctionLikeStorage $storage): string {
    $storage->return_type = "x";

    if ($storage instanceof MethodStorage) {
        $storage->has_docblock_return_type = true;
    }

    return $storage->return_type;
}
