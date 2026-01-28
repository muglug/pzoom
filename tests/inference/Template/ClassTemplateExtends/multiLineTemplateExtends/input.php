<?php
interface IdInterface {}

/**
 * @template D of array<string, scalar|null>
 */
interface WriteModelInterface
{
    /**
     * Returns the values to be stored when saving.
     *
     * @psalm-return D
     */
    public function valuesToSave(): array;
}

/**
 * @psalm-immutable
 * @template D of array<string, scalar|null>
 * @implements WriteModelInterface<D>
 */
abstract class WriteModel implements WriteModelInterface
{
}

/**
 * @extends WriteModelInterface<
 *     array{
 *         ulid: string,
 *         senderPersonId: int
 *     }
 * >
 */
interface EmailWriteModelInterface extends WriteModelInterface
{
}

/**
 * @psalm-immutable
 * @extends WriteModel<array{
 *    ulid: string,
 *    senderPersonId: int
 * }>
 */
final class EmailWriteModel extends WriteModel implements EmailWriteModelInterface
{
    public function valuesToSave(): array
    {
        return [
            "ulid" => "a string",
            "senderPersonId" => 1,
        ];
    }
}