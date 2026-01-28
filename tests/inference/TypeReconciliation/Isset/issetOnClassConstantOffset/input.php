<?php

final class StudyJwtPayload {
    public const STUDY_ID = "studid";

    public static function fromClaims(array $claims): string
    {
        if (!isset($claims["usrid"])) {
            throw new \InvalidArgumentException();
        }

        if (!\is_string($claims["usrid"])) {
            throw new \InvalidArgumentException();
        }

        if (!isset($claims[self::STUDY_ID])) {
            throw new \InvalidArgumentException();
        }

        if (!\is_string($claims[self::STUDY_ID])) {
            throw new \InvalidArgumentException();
        }

        return $claims[self::STUDY_ID];
    }
}