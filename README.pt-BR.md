# base64 para Windows

[English](README.md) | **Português (Brasil)**

Comando `base64` nativo para Windows, escrito em Rust e sem dependências, com as mesmas flags, mensagens de erro e códigos de saída do GNU coreutils 8.32 no Linux.

Scripts que usam `base64` no Linux funcionam igual, sem precisar de WSL, Git Bash ou MSYS2.

```console
$ echo -n "olá, mundo" | base64
b2zDoSwgbXVuZG8=

$ echo b2zDoSwgbXVuZG8= | base64 -d
olá, mundo
```

## Instalação

É preciso ter o [Rust](https://rustup.rs) e o linker do MSVC, que vem com as Build Tools do Visual Studio.

```console
git clone https://github.com/pablorobert/base64-rs.git
cd base64-rs
cargo install --path .
```

O executável vai para `%USERPROFILE%\.cargo\bin\base64.exe`. Se outro `base64` já estiver no `PATH`, como o do Git, confira qual deles é usado com `where base64`.

Para apenas compilar, use `cargo build --release`. O binário fica em `target\release\base64.exe`.

## Uso

```text
base64 [OPÇÃO]... [ARQUIVO]
```

Sem `ARQUIVO`, ou quando `ARQUIVO` é `-`, lê da entrada padrão.

| Opção | Descrição |
| --- | --- |
| `-d`, `--decode` | Decodifica os dados |
| `-i`, `--ignore-garbage` | Na decodificação, ignora caracteres fora do alfabeto |
| `-w`, `--wrap=COLS` | Quebra as linhas codificadas a cada `COLS` caracteres (padrão: 76; `0` desativa) |
| `--help` | Mostra a ajuda e sai |
| `--version` | Mostra a versão e sai |

As opções seguem as regras do `getopt_long`: podem ser agrupadas (`-di`), abreviadas (`--dec`) e misturadas com o arquivo (`base64 arquivo.b64 -d`). `--` encerra as opções.

### Exemplos

```console
# Codificar um arquivo em uma única linha
base64 -w0 foto.png

# Decodificar para um arquivo
base64 -d foto.b64 > foto.png

# Quebrar as linhas a cada 4 caracteres
base64 -w4 arquivo.txt

# Decodificar um arquivo com quebras de linha CRLF
base64 -d arquivo-windows.b64
```

Em caso de erro, a saída é `1`. Com dados inválidos, tudo o que era válido antes do erro é escrito, seguido de `base64: invalid input` em stderr, como no GNU.

## Cuidados no Windows

- **Espaço e CRLF no fim da linha:** diferente do GNU, `-d` aceita espaços e `\r` no fim de uma linha ou da entrada, porque o `echo` do `cmd` envia `texto= \r\n` em `echo texto= | base64 -d`. No meio dos dados, eles ainda geram `invalid input`. Use `-di` para descartá-los em qualquer posição.
- **Windows PowerShell 5.1:** `>` e `|` entre comandos nativos recodificam os bytes e podem corromper dados binários. Prefira o `cmd`, o Git Bash ou o PowerShell 7.4+.
- **Saída no console:** `-d` escreve os bytes sem conversão. Se os acentos aparecerem errados, ative UTF-8 com `chcp 65001`.
- **Interop do WSL:** executar um `.exe` a partir do WSL com stdin e stdout em arquivos grandes pode travar. Isso também aconteceu com o `base64.exe` do Git, então a causa não está neste projeto. Rode o binário direto no Windows.

## Diferenças em relação ao GNU

| Situação | GNU no Linux | Este projeto |
| --- | --- | --- |
| `--version` | Texto do coreutils | `base64 0.1.0` |
| `--help` | Inclui links do GNU no rodapé | Mesmo texto, sem o rodapé |
| Aspas em mensagens de erro | Dependem do locale (`‘x’` em UTF-8) | Sempre `'x'`, como em `LC_ALL=C` |
| Pipe fechado (`base64 grande \| head`) | Encerrado por `SIGPIPE` | Sai em silêncio com código `1` |

## Testes

```console
cargo test
```

A suíte em `tests/cli.rs` cobre os vetores da RFC 4648, quebra de linha, decodificação, mensagens de erro e dados binários que cruzam os limites do buffer.

A versão inicial também foi comparada com o `base64` do GNU coreutils 8.32 no Ubuntu 22.04: os 420 casos tiveram stdout, stderr e código de saída idênticos, com `LC_ALL=C`. Eles incluem binários aleatórios de até 1 MB, entradas malformadas e erros de opções. O script dessa comparação não faz parte do repositório, e o uso em uma janela de console real não foi testado.

## Licença

Distribuído sob a [licença MIT](LICENSE).
