<?php
/**
 * @template A
 */
final class ArrayList
{
    /**
     * @template B
     * @param Closure(A): B $ab
     * @return ArrayList<B>
     */
    public function map(Closure $ab): ArrayList
    {
        throw new RuntimeException("???");
    }
}

/**
 * @template T
 * @param ArrayList<T> $list
 * @return ArrayList<array{T}>
 */
function asTupled(ArrayList $list): ArrayList
{
    return $list->map(function ($_a) {
        return [$_a];
    });
}
/** @var ArrayList<int> $a */
$a = new ArrayList();
$b = asTupled($a);
