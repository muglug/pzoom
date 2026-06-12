<?php
final class NA {
    /**
     * @param non-empty-string $name
     * @return non-empty-string , e.g. 'Psalm'
     * @psalm-pure
     */
    public static function getNameSpaceRoot(string $name): string
    {
        $parts = explode('\\', ltrim($name, '\\'));
        return $parts[0] === '' ? 'x' : $parts[0];
    }
}
final class Storage6 {
    /** @var list<non-empty-string> */
    public array $internal = [];
}
function f(Storage6 $s): void {
    $s->internal = [NA::getNameSpaceRoot('Psalm\Internal')];
}
