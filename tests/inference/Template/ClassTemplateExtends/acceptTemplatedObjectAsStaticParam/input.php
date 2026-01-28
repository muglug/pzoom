<?php
/**
 * @psalm-immutable
 */
abstract class Id
{
    protected string $id;

    final protected function __construct(string $id)
    {
        $this->id = $id;
    }

    /**
     * @param static $id
     */
    final public function equals(self $id): bool
    {
        return $this->id === $id->id;
    }
}

/**
 * @template T of Id
 */
final class Ids
{
    /**
     * @psalm-var list<T>
     */
    private array $ids;

    /**
     * @psalm-param list<T> $ids
     */
    private function __construct(array $ids)
    {
        $this->ids = $ids;
    }

    /**
     * @psalm-param T $id
     */
    public function contains(Id $id): bool
    {
        foreach ($this->ids as $oneId) {
            if ($oneId->equals($id)) {
                return true;
            }
        }

        return false;
    }
}