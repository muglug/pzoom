<?php

final class Handler
{
    /**
     * @var class-string<Throwable>[]
     */
    private array $dontReport = [];

    /**
     * @param class-string<Throwable> $throwable
     */
    public function dontReport(string $throwable): void
    {
        $this->dontReport[] = $throwable;
    }

    public function shouldReport(Throwable $t): bool
    {
        foreach ($this->dontReport as $tc) {
            if ($t instanceof $tc) {
                return false;
            }
        }

        return true;
    }
}

$h = new Handler();
$h->dontReport(RuntimeException::class);

$h->shouldReport(new Exception());
$h->shouldReport(new RuntimeException());