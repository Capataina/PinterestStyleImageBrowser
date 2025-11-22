# Tech Context

## Technology Stack

### Frontend Technologies

#### Core Framework

- **React 19**: Latest React with concurrent features and improved performance
- **TypeScript**: Full type safety across the frontend codebase
- **Vite 7**: Lightning-fast build tool with hot module replacement

#### Styling

- **Tailwind CSS v4**: Utility-first CSS framework with native Vite plugin
- **shadcn/ui**: Planned component library built on Radix UI primitives
- **CSS Modules**: For component-specific styles when needed

#### State Management

- **Zustand**: Planned lightweight state management for complex application state
- **React Hooks**: Built-in hooks for local component state

#### Animation

- **TBD**: Either framer-motion or react-spring for smooth UI animations
  - framer-motion: More declarative, easier to use, great for UI transitions
  - react-spring: More performant, physics-based animations

### Backend Technologies

#### Core Framework

- **Tauri v2**: Modern desktop application framework combining Rust backend with web frontend
- **Rust**: Systems programming language providing memory safety and performance

#### Database

- **SQLite**: Embedded relational database via rusqlite
- **rusqlite**: Rust bindings for SQLite with excellent ergonomics

#### File System

- **Standard Library**: Rust std::fs for file operations
- **Walkdir** (potential): For efficient directory traversal

### Future AI/ML Technologies

#### Semantic Search & Recommendations

- **ONNX Runtime**: Cross-platform ML inference engine
- **CLIP Model**: OpenAI's vision-language model for image embeddings
- **Vector Search**: Approximate nearest neighbor search for similarity
  - Options: FAISS, hnswlib, or custom implementation

## Development Environment

### Required Tools

#### System Requirements

- **Operating System**: Windows 10/11, macOS 10.15+, or Linux
- **Architecture**: x64 (AMD64) or ARM64

#### Rust Toolchain

- **Rust**: Latest stable version (1.70+)
- **Cargo**: Rust package manager (comes with Rust)
- **Toolchain**:
  - Windows: MSVC toolchain (stable-x86_64-pc-windows-msvc)
  - macOS/Linux: Default stable toolchain

#### Node.js Environment

- **Node.js**: v18 or v20 (LTS versions recommended)
- **Package Manager**: npm (included with Node.js)
- **Alternative**: pnpm or yarn also work

#### Build Tools

- **Windows**: Visual Studio Build Tools or Visual Studio with C++ workload
- **macOS**: Xcode Command Line Tools
- **Linux**: build-essential package (gcc, g++, make)

### Project Setup

#### Initial Installation

1. **Clone Repository**

   - Git clone from project repository

2. **Install Dependencies**

   - Frontend: `npm install` in project root
   - Backend: Dependencies auto-installed via Cargo

3. **Run Development Build**
   - `npm run tauri dev` starts both frontend and backend in dev mode

#### Project Structure

```
PinterestStyleImageBrowser/
├── src/                    # Frontend source
│   ├── components/         # React components
│   ├── stores/            # Zustand stores
│   ├── hooks/             # Custom React hooks
│   ├── types/             # TypeScript type definitions
│   └── App.tsx            # Main app component
├── src-tauri/             # Backend source
│   ├── src/               # Rust source files
│   │   ├── lib.rs         # Main entry point
│   │   ├── db.rs          # Database operations
│   │   ├── filesystem.rs  # File scanning
│   │   └── image_struct.rs # Data structures
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
├── memory-bank/           # Project documentation
├── package.json           # Node dependencies
└── vite.config.ts         # Vite configuration
```

### Configuration Files

#### Tauri Configuration

- **Location**: `src-tauri/tauri.conf.json`
- **Purpose**: App metadata, window settings, security policies
- **Key Settings**:
  - Asset protocol scope for file access
  - App identifier and version
  - Window dimensions and behavior
  - Build targets and icons

#### Vite Configuration

- **Location**: `vite.config.ts`
- **Purpose**: Frontend build configuration
- **Key Settings**:
  - Tailwind CSS v4 plugin integration
  - React plugin configuration
  - Tauri-specific build settings

#### TypeScript Configuration

- **Locations**: `tsconfig.json`, `tsconfig.node.json`
- **Purpose**: TypeScript compiler options
- **Key Settings**:
  - Strict type checking enabled
  - Module resolution settings
  - Path aliases for imports

#### Cargo Configuration

- **Location**: `src-tauri/Cargo.toml`
- **Purpose**: Rust dependencies and build settings
- **Key Dependencies**:
  - tauri: Core framework
  - rusqlite: SQLite database
  - serde: Serialization for IPC
  - tokio (future): Async runtime

## Development Workflow

### Running the Application

#### Development Mode

- **Command**: `npm run tauri dev`
- **Features**:
  - Hot reload for frontend changes
  - Fast incremental Rust compilation
  - DevTools accessible in app window
  - Console logging enabled

#### Production Build

- **Command**: `npm run tauri build`
- **Output**: Platform-specific installer in `src-tauri/target/release/bundle/`
- **Optimizations**: Minified frontend, optimized Rust binary

### Common Development Tasks

#### Adding Frontend Dependencies

```bash
npm install [package-name]
```

#### Adding Backend Dependencies

Edit `src-tauri/Cargo.toml` and add to dependencies section

#### Database Management

- Database file: `src-tauri/images.db`
- Recreate database: Delete images.db file
- Schema changes: Requires migration implementation

#### Debugging

- Frontend: Browser DevTools in app window
- Backend: Use `println!` or `dbg!` macros, logs appear in terminal

## Technical Constraints

### Platform-Specific Considerations

#### Windows

- Requires MSVC toolchain (not GNU)
- Path separators: backslashes
- Case-insensitive filesystem
- Asset protocol URLs use forward slashes

#### macOS

- Code signing required for distribution
- Gatekeeper restrictions
- Case-sensitive filesystem option affects path handling

#### Linux

- Multiple distribution formats (AppImage, .deb, .rpm)
- Dependency variations across distributions
- X11 vs Wayland considerations

### Performance Constraints

#### Memory

- Each image in grid requires memory for DOM element
- Virtual scrolling needed for 10,000+ images
- Rust backend more memory-efficient than frontend

#### CPU

- Image decoding handled by browser
- Parallel processing available in Rust
- Database operations generally fast with proper indexing

#### Storage

- Database size grows with image count and tags
- Indexes increase database size but improve query speed
- Future: Embeddings will significantly increase storage needs

### Security Constraints

#### Tauri Security Model

- WebView sandboxed from system
- Explicit permissions required for file access
- IPC commands are the only communication bridge
- CSP headers restrict what frontend can do

#### File System Access

- Must configure asset protocol scope
- Path validation required in commands
- No direct filesystem access from frontend

## Build and Deployment

### Build Process

#### Frontend Build

1. Vite bundles React app
2. TypeScript compiled to JavaScript
3. Tailwind CSS processed and optimized
4. Assets copied to dist folder

#### Backend Build

1. Cargo compiles Rust code
2. Dependencies statically linked
3. Tauri integrates frontend bundle
4. Platform-specific binary produced

### Distribution

#### Windows

- MSI installer (recommended)
- NSIS installer (alternative)
- Portable executable

#### macOS

- DMG disk image
- App bundle
- Code signing required for notarization

#### Linux

- AppImage (universal)
- .deb package (Debian/Ubuntu)
- .rpm package (Fedora/RHEL)

## Development Best Practices

### Code Organization

- Keep components small and focused
- Separate business logic from UI
- Use TypeScript types consistently
- Follow Rust naming conventions

### Performance

- Lazy load images with intersection observer
- Debounce search input
- Memoize expensive computations
- Use React.memo for pure components

### Testing

- Write unit tests for complex logic
- Test Rust functions independently
- Mock Tauri IPC in frontend tests
- Integration tests for critical flows

### Version Control

- Commit message conventions
- Branch strategy for features
- Tag releases for distribution
- .gitignore properly configured

## Troubleshooting Common Issues

### Build Failures

#### Rust Compilation Errors

- Ensure correct toolchain installed
- Update dependencies in Cargo.toml
- Clear target folder: `cargo clean`

#### Frontend Build Errors

- Delete node_modules and reinstall
- Check Node.js version compatibility
- Verify package.json dependencies

### Runtime Issues

#### Images Not Loading (403)

- Check asset protocol scope configuration
- Verify file paths are absolute
- Confirm files exist at specified paths

#### Database Errors

- Delete database to reset
- Check file permissions
- Verify SQLite is properly initialized

#### Performance Issues

- Check for memory leaks in DevTools
- Profile Rust code with cargo flamegraph
- Reduce number of rendered images

## Future Technical Considerations

### Upcoming Integrations

#### shadcn/ui Setup

- Install Radix UI dependencies
- Configure Tailwind for component styles
- Set up component directory structure
- Import and customize components

#### Animation Library Integration

- Install chosen library (framer-motion or react-spring)
- Create animation presets
- Integrate with component transitions
- Performance test with many animations

#### Zustand State Management

- Define store structures
- Implement state slicing
- Add persistence middleware
- Connect to React components

### ML/AI Pipeline

#### CLIP Integration

- Add onnxruntime dependency
- Download CLIP model files
- Implement embedding generation
- Optimize batch processing

#### Vector Search

- Choose vector database or library
- Implement similarity computation
- Add indexing for fast lookup
- Integrate with UI for recommendations

## Resources and Documentation

### Official Documentation

- Tauri: https://tauri.app/
- React: https://react.dev/
- Rust: https://www.rust-lang.org/learn
- Vite: https://vitejs.dev/
- Tailwind CSS: https://tailwindcss.com/

### Community Resources

- Tauri Discord: Community support
- Stack Overflow: Technical questions
- GitHub Discussions: Feature requests and Q&A
- Reddit: r/tauri, r/rust, r/reactjs

### Learning Resources

- Rust Book: Comprehensive Rust guide
- React Docs: Modern React patterns
- TypeScript Handbook: Type system deep dive
- Tauri Examples: Sample applications
