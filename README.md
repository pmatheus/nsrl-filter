# NSRL Filter

Uma ferramenta de alto desempenho para filtrar listas de arquivos comparando-as com o banco de dados da National Software Reference Library (NSRL), a fim de identificar softwares conhecidos e desconhecidos.

## Visão Geral

O NSRL Filter é um utilitário desenvolvido em Rust que compara valores de hash de arquivos (SHA-1 e MD5) de um arquivo CSV com o banco de dados NSRL para separar softwares conhecidos de arquivos desconhecidos. Esta ferramenta é particularmente útil em fluxos de trabalho de forense digital e análise de malware.

## Funcionalidades

- **Alto Desempenho**: Utiliza processamento em lote e consultas otimizadas ao banco de dados SQLite para buscas rápidas de hash.
- **Indexação Automática**: Cria e utiliza índices nas colunas de hash (SHA-1, MD5) do banco de dados para acelerar as consultas.
- **Visualização de Progresso**: Barras de progresso em tempo real e atualizações de status durante o processamento.
- **Eficiência de Memória**: Processa grandes conjuntos de dados com um consumo mínimo de memória.
- **Entrada/Saída Flexível**: Funciona com formatos de arquivo CSV padrão e permite a especificação de caminhos para o banco de dados e a lista de arquivos.
- **Detecção de Tabela**: Identifica automaticamente as tabelas `METADATA` ou `FILE` dentro do banco de dados NSRL.
- **Relatórios Detalhados**: Fornece um resumo ao final do processamento, incluindo contagens de arquivos conhecidos, desconhecidos, com hashes vazios e duplicados.

## Instalação

### Pré-requisitos

- Rust e Cargo (versão estável mais recente recomendada)

### Compilando a Partir do Código Fonte

```bash
# Clone o repositório (substitua pela URL correta, se necessário)
# git clone https://github.com/pmatheus/nsrl-filter.git
# cd nsrl-filter

# Compile em modo de release para desempenho otimizado
cargo build --release
```

O binário compilado estará disponível em `target/release/nsrl-filter` (ou `target\release\nsrl-filter.exe` no Windows).

## Uso

```bash
# Uso básico (assume que 'nsrl.db' e 'filelist.csv' estão no diretório atual)
nsrl-filter

# Especificar caminhos personalizados para o banco de dados e a lista de arquivos
nsrl-filter caminho/para/seu/nsrl.db caminho/para/sua/filelist.csv
```

### Formato do Arquivo de Entrada

_IMPORTANTE: O arquivo CSV de entrada deve seguir a ordem das colunas usada pelo RDS da NSRL, especialmente para os hashes. A ferramenta espera que a coluna de hash MD5 esteja no índice 6 (sétima coluna) e a coluna de hash SHA-1 no índice 7 (oitava coluna)._

A ferramenta espera um arquivo CSV com cabeçalhos. Se a sua lista de arquivos não segue essa ordem, você precisará ajustá-la.

Exemplo de cabeçalhos esperados (a ordem das colunas de hash é crucial):
`"ProductName","ProductVersion","ApplicationType","OSCode","MfgCode","Language","MD5","SHA-1","FileName","FileSize","SpecialCode"`

### Saída

A ferramenta gera dois arquivos CSV no mesmo diretório do arquivo CSV de entrada:
- `<nome_do_arquivo_de_entrada>_known.csv`: Arquivos que correspondem a entradas no banco de dados NSRL.
- `<nome_do_arquivo_de_entrada>_unknown.csv`: Arquivos que não correspondem a nenhuma entrada no banco de dados NSRL.

## Notas de Desempenho

- A primeira execução em um novo banco de dados pode levar algum tempo para criar os índices, o que acelera significativamente as execuções subsequentes.
- O desempenho depende do tamanho do banco de dados e do número de valores de hash únicos na lista de arquivos de entrada.

## Esquema do Banco de Dados (Exemplo NSRL)

A ferramenta é projetada para funcionar com bancos de dados SQLite derivados do NSRL RDS. Ela procura por tabelas chamadas `METADATA` ou `FILE` que contenham colunas `sha1` e/ou `md5`.

Um exemplo da estrutura da tabela `FILE` que é compatível:

```sql
CREATE TABLE FILE (
    SHA1 TEXT,
    MD5 TEXT,
    -- outras colunas relevantes do NSRL
    FileName TEXT,
    FileSize INTEGER
    -- etc.
);
```

A ferramenta também pode usar uma tabela `METADATA` com estrutura similar para `sha1` e `md5`.

## Licença

[Licença MIT](LICENSE) (Se o arquivo LICENSE existir no seu projeto)

## Contribuições

Contribuições são bem-vindas! Sinta-se à vontade para enviar um Pull Request.