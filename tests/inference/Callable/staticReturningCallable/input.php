<?php
abstract class Id
{
    /**
     * @var string
     */
    private $id;

    final protected function __construct(string $id)
    {
        $this->id = $id;
    }

    /**
     * @return static
     */
    final public static function fromString(string $id): self
    {
        return new static($id);
    }
}

final class CriterionId extends Id
{
}

final class CriterionIds
{
    /**
     * @psalm-var non-empty-list<CriterionId>
     */
    private $ids;

    /**
     * @psalm-param non-empty-list<CriterionId> $ids
     */
    private function __construct(array $ids)
    {
        $this->ids = $ids;
    }

    /**
     * @psalm-param non-empty-list<string> $ids
     */
    public static function fromStrings(array $ids): self
    {
        return new self(array_map([CriterionId::class, "fromString"], $ids));
    }
}
