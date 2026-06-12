<?php
/** @var int-mask<GLOB_NOCHECK> */
$maybeNocheckFlag = 0;
/** @var int-mask<GLOB_ONLYDIR> */
$maybeOnlydirFlag = 0;

/** @var string */
$string = '';

$emptyPatternNoFlags = glob( '' );
$emptyPatternWithoutNocheckFlag1 = glob( '', GLOB_MARK );
$emptyPatternWithoutNocheckFlag2 = glob( '' , GLOB_NOSORT | GLOB_NOESCAPE);
$emptyPatternWithoutNocheckFlag3 = glob( '' , GLOB_MARK | GLOB_NOSORT  | GLOB_NOESCAPE | GLOB_BRACE | GLOB_ONLYDIR | GLOB_ERR);
$emptyPatternWithNocheckFlag1 = glob( ''  , GLOB_NOCHECK);
$emptyPatternWithNocheckFlag2 = glob( '' , GLOB_NOCHECK | GLOB_MARK);
$emptyPatternWithNocheckFlag3 = glob( '' , GLOB_NOCHECK | GLOB_MARK | GLOB_NOSORT | GLOB_NOESCAPE | GLOB_BRACE | GLOB_ERR);
$emptyPatternWithNocheckAndOnlydirFlag1 = glob( '' , GLOB_NOCHECK | GLOB_ONLYDIR);
$emptyPatternWithNocheckAndOnlydirFlag2 = glob( '' , GLOB_NOCHECK | GLOB_ONLYDIR | GLOB_MARK);
$emptyPatternWithNocheckAndOnlydirFlag3 = glob( '' , GLOB_NOCHECK | GLOB_ONLYDIR | GLOB_MARK | GLOB_NOSORT | GLOB_NOESCAPE | GLOB_BRACE | GLOB_ERR);
$emptyPatternWithNocheckFlagAndMaybeOnlydir = glob( '' , GLOB_NOCHECK | $maybeOnlydirFlag);
$emptyPatternMaybeWithNocheckFlag = glob( '' , $maybeNocheckFlag);
$emptyPatternMaybeWithNocheckFlagAndOnlydir = glob( '' , $maybeNocheckFlag | GLOB_ONLYDIR);
$emptyPatternMaybeWithNocheckFlagAndMaybeOnlydir = glob( '' , $maybeNocheckFlag | $maybeOnlydirFlag);

$nonEmptyPatternNoFlags = glob( 'pattern' );
$nonEmptyPatternWithoutNocheckFlag1 = glob( 'pattern', GLOB_MARK );
$nonEmptyPatternWithoutNocheckFlag2 = glob( 'pattern' , GLOB_NOSORT | GLOB_NOESCAPE);
$nonEmptyPatternWithoutNocheckFlag3 = glob( 'pattern' , GLOB_MARK | GLOB_NOSORT  | GLOB_NOESCAPE | GLOB_BRACE | GLOB_ONLYDIR | GLOB_ERR);
$nonEmptyPatternWithNocheckFlag1 = glob( 'pattern'  , GLOB_NOCHECK);
$nonEmptyPatternWithNocheckFlag2 = glob( 'pattern' , GLOB_NOCHECK | GLOB_MARK);
$nonEmptyPatternWithNocheckFlag3 = glob( 'pattern' , GLOB_NOCHECK | GLOB_MARK | GLOB_NOSORT | GLOB_NOESCAPE | GLOB_BRACE | GLOB_ERR);
$nonEmptyPatternWithNocheckAndOnlydirFlag1 = glob( 'pattern' , GLOB_NOCHECK | GLOB_ONLYDIR);
$nonEmptyPatternWithNocheckAndOnlydirFlag2 = glob( 'pattern' , GLOB_NOCHECK | GLOB_ONLYDIR | GLOB_MARK);
$nonEmptyPatternWithNocheckAndOnlydirFlag3 = glob( 'pattern' , GLOB_NOCHECK | GLOB_ONLYDIR | GLOB_MARK | GLOB_NOSORT | GLOB_NOESCAPE | GLOB_BRACE | GLOB_ERR);
$nonEmptyPatternWithNocheckFlagAndMaybeOnlydir = glob( 'pattern' , GLOB_NOCHECK | $maybeOnlydirFlag);
$nonEmptyPatternMaybeWithNocheckFlag = glob( 'pattern' , $maybeNocheckFlag);
$nonEmptyPatternMaybeWithNocheckFlagAndOnlydir = glob( 'pattern' , $maybeNocheckFlag | GLOB_ONLYDIR);
$nonEmptyPatternMaybeWithNocheckFlagAndMaybeOnlydir = glob( 'pattern' , $maybeNocheckFlag | $maybeOnlydirFlag);

$stringPatternNoFlags = glob( $string );
$stringPatternWithoutNocheckFlag1 = glob( $string, GLOB_MARK );
$stringPatternWithoutNocheckFlag2 = glob( $string , GLOB_NOSORT | GLOB_NOESCAPE);
$stringPatternWithoutNocheckFlag3 = glob( $string , GLOB_MARK | GLOB_NOSORT  | GLOB_NOESCAPE | GLOB_BRACE | GLOB_ONLYDIR | GLOB_ERR);
$stringPatternWithNocheckFlag1 = glob( $string  , GLOB_NOCHECK);
$stringPatternWithNocheckFlag2 = glob( $string , GLOB_NOCHECK | GLOB_MARK);
$stringPatternWithNocheckFlag3 = glob( $string , GLOB_NOCHECK | GLOB_MARK | GLOB_NOSORT | GLOB_NOESCAPE | GLOB_BRACE | GLOB_ERR);
$stringPatternWithNocheckAndOnlydirFlag1 = glob( $string , GLOB_NOCHECK | GLOB_ONLYDIR);
$stringPatternWithNocheckAndOnlydirFlag2 = glob( $string , GLOB_NOCHECK | GLOB_ONLYDIR | GLOB_MARK);
$stringPatternWithNocheckAndOnlydirFlag3 = glob( $string , GLOB_NOCHECK | GLOB_ONLYDIR | GLOB_MARK | GLOB_NOSORT | GLOB_NOESCAPE | GLOB_BRACE | GLOB_ERR);
$stringPatternWithNocheckFlagAndMaybeOnlydir = glob( $string , GLOB_NOCHECK | $maybeOnlydirFlag);
$stringPatternMaybeWithNocheckFlag = glob( $string , $maybeNocheckFlag);
$stringPatternMaybeWithNocheckFlagAndOnlydir = glob( $string , $maybeNocheckFlag | GLOB_ONLYDIR);
$stringPatternMaybeWithNocheckFlagAndMaybeOnlydir = glob( $string , $maybeNocheckFlag | $maybeOnlydirFlag);
